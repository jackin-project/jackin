// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use jackin_usage_provider_core::{
    Auth, Decimal, Exact, Http, HubAddress, HubProvider, ProviderError,
};

const NOT_JSON: ProviderError = ProviderError::Decode("the response is not valid JSON");

fn parsed(text: &str) -> Result<Exact, ProviderError> {
    Exact::parse(text.as_bytes())
}

fn promptly<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    let (sender, receiver) = mpsc::channel();
    let _worker = thread::spawn(move || {
        let _sent = sender.send(work());
    });
    receiver
        .recv_timeout(Duration::from_secs(20))
        .map_err(|error| error.to_string())
}

#[test]
fn number_leaves_keep_the_text_the_server_sent() -> Result<(), ProviderError> {
    let body = parsed(
        r#"{"a":0.10000000000000001,"b":[1e-7,"x",null,true],"c":{"d":-0,"e":" 2.5e-3 "},"kéy":"é"}"#,
    )?;
    let amount = |value: &Exact| {
        value
            .decimal()
            .map(|found| found.map(|exact| exact.to_string()))
    };
    assert_eq!(amount(&body["a"])?, Some("0.10000000000000001".to_owned()));
    assert_eq!(amount(&body["c"]["d"])?, Some("0".to_owned()));
    assert_eq!(amount(&body["c"]["e"])?, Some("0.0025".to_owned()));
    let list = body["b"].as_list().ok_or(NOT_JSON)?;
    assert_eq!(list.len(), 4);
    assert_eq!(amount(&list[0])?, Some("0.0000001".to_owned()));
    assert_eq!(list[1].as_str(), Some("x"));
    assert!(list[2].is_null());
    assert_eq!(list[3].as_bool(), Some(true));
    assert_eq!(body["kéy"].as_str(), Some("é"));
    assert!(body["c"].is_object());
    assert!(!body["a"].is_object());
    Ok(())
}

#[test]
fn an_absent_member_or_a_member_of_a_scalar_reads_as_null() -> Result<(), ProviderError> {
    let body = parsed(r#"{"a":1,"list":[1]}"#)?;
    assert!(body["missing"].is_null());
    assert!(body["a"]["deeper"].is_null());
    assert!(body["list"]["key"].is_null());
    assert_eq!(body["missing"]["deeper"].decimal(), Ok(None));
    assert_eq!(parsed(r#"{"a":1,"a":2}"#)?["a"], parsed("2")?);
    Ok(())
}

#[test]
fn only_numbers_and_numeric_text_are_amounts() -> Result<(), ProviderError> {
    let body = parsed(
        r#"{"n":1e999,"s":"1e999","t":"abc","e":"","b":true,"z":null,"l":[1],"o":{},"ok":"  3.25 "}"#,
    )?;
    for member in ["n", "s"] {
        assert_eq!(
            body[member].decimal(),
            Err(ProviderError::OUT_OF_RANGE),
            "{member}"
        );
    }
    for member in ["t", "e", "b", "z", "l", "o", "missing"] {
        assert_eq!(body[member].decimal(), Ok(None), "{member}");
    }
    assert_eq!(
        body["ok"]
            .decimal()
            .map(|found| found.map(|amount| amount.to_string())),
        Ok(Some("3.25".to_owned()))
    );
    assert_eq!(
        Decimal::parse_text("3.25").map(|amount| amount.to_string()),
        Ok("3.25".to_owned())
    );
    Ok(())
}

#[test]
fn text_that_is_not_one_json_value_is_a_decode_error() {
    for text in [
        "",
        " ",
        "{",
        "}",
        "{\"a\":1}x",
        "{\"a\":1}{}",
        "[1,]",
        "01",
        "1.",
        "{\"a\":}",
        "\"open",
        "nul",
        "NaN",
        "{'a':1}",
    ] {
        assert_eq!(parsed(text), Err(NOT_JSON), "{text:?}");
    }
    assert_eq!(Exact::parse(&[0x7b, 0xff, 0x7d]), Err(NOT_JSON));
}

#[test]
fn nesting_beyond_the_limit_is_an_error_not_a_stack_overflow() {
    let nested = |depth: usize| format!("{}1{}", "[".repeat(depth), "]".repeat(depth));
    assert_eq!(parsed(&nested(16)).map(|_| ()), Ok(()));
    assert_eq!(parsed(&nested(17)), Err(NOT_JSON));
    let found = promptly(move || {
        (
            Exact::parse(nested(10_000).as_bytes()),
            Exact::parse("[".repeat(500_000).as_bytes()),
            Exact::parse(format!("{}1{}", "{\"a\":".repeat(5_000), "}".repeat(5_000)).as_bytes()),
        )
    });
    assert_eq!(found, Ok((Err(NOT_JSON), Err(NOT_JSON), Err(NOT_JSON))));
}

#[test]
fn brackets_inside_text_do_not_count_as_nesting() -> Result<(), ProviderError> {
    let brackets = "[".repeat(20);
    let body = parsed(&format!("{{\"a\":\"{brackets}\\\"{brackets}\",\"b\":[1]}}"))?;
    assert_eq!(body["b"].as_list().map(<[Exact]>::len), Some(1));
    assert_eq!(body["a"].as_str().map(str::len), Some(41));
    let after_a_backslash = format!(
        "{{\"a\":\"x\\\\\",\"b\":{}1{}}}",
        "[".repeat(17),
        "]".repeat(17)
    );
    assert_eq!(parsed(&after_a_backslash), Err(NOT_JSON));
    Ok(())
}

#[test]
fn debug_output_hides_the_body() -> Result<(), ProviderError> {
    let shown = format!(
        "{:?}",
        parsed(r#"{"label":"sk-or-v1-private","limit":1.5}"#)?
    );
    for hidden in ["label", "sk-or-v1-private", "1.5", "limit"] {
        assert!(!shown.contains(hidden), "{shown}");
    }
    Ok(())
}

#[test]
fn a_body_of_a_mebibyte_at_the_deepest_level_is_parsed_promptly() {
    let found = promptly(|| {
        let leaf = "x".repeat(1_000_000);
        let body = format!("{}\"{leaf}\"{}", "{\"k\":".repeat(16), "}".repeat(16));
        Exact::parse(body.as_bytes()).map(|parsed| {
            let mut at = &parsed;
            for _ in 0..16 {
                at = &at["k"];
            }
            at.as_str().map(str::len)
        })
    });
    assert_eq!(found, Ok(Ok(Some(1_000_000))));
}

#[tokio::test]
async fn a_hub_login_cannot_ask_for_exact_amounts() -> Result<(), ProviderError> {
    let hub = HubAddress::parse("http://127.0.0.1:9")?;
    let http = Http::new(Duration::from_secs(2)).routed_via_hub(
        hub,
        "management-key".into(),
        "auth".into(),
        HubProvider::Grok,
    );
    assert_eq!(
        http.get_exact("https://openrouter.ai/api/v1/key", Auth::Bearer("key"), &[])
            .await,
        Err(ProviderError::Unsupported(
            "a hub login cannot return exact amounts"
        ))
    );
    Ok(())
}
