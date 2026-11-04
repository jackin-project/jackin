// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin::ExitClass;
use jackin_core::Provider;
use jackin_testkit::{Method, Route, TestResult, World, guard};
use tokio_util::sync::CancellationToken;

use super::support::{CLOSED_ORIGIN, Failure, FixedConnector, Harness};

const WORK: &str = include_str!("fixtures/usage/usage_cli_work.json");
const CACHED: &str = include_str!("fixtures/usage/usage_cli_cached.json");
const ONE: &str = include_str!("fixtures/usage/usage_cli_one.json");
const AUTH: &str = include_str!("fixtures/usage/auth_chatgpt.json");
const KEY: &str = include_str!("fixtures/usage/openrouter_key.json");
const WHAM: &str = "/backend-api/wham/usage";

fn login_account(world: &World, name: &str, account_id: &str) -> Result<String, Failure> {
    let home = world.home().as_path().join(name);
    std::fs::create_dir_all(&home)?;
    std::fs::write(home.join("auth.json"), AUTH)?;
    Ok(format!(
        "[accounts.{name}]\nprovider = \"codex\"\ncredential = {{ source = \"codex-login\", home = \"{}\", chatgpt_account_id = \"{account_id}\" }}\n",
        home.display()
    ))
}

fn env_account(name: &str, provider: &str, var: &str) -> String {
    format!(
        "[accounts.{name}]\nprovider = \"{provider}\"\ncredential = {{ source = \"env\", var = \"{var}\" }}\n"
    )
}

fn codex_route(body: &str, account_id: &str) -> Route {
    Route::json(200, body)
        .requires_header("authorization", "Bearer SECRET_TOKEN_CODEX")
        .requires_header("chatgpt-account-id", account_id)
}

async fn started(config: &str, world: &World) -> Result<Harness, Failure> {
    let server = world.usage.get().await?;
    let mut harness = Harness::new(world);
    harness.connector = FixedConnector::empty();
    harness.usage_endpoints = server.endpoints();
    harness.write_config(config)?;
    Ok(harness)
}

#[tokio::test]
async fn usage_prints_one_line_per_window_in_account_order_and_marks_stale_and_unavailable()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let config = [
            login_account(&world, "work", "acct-work")?,
            login_account(&world, "cached", "acct-cached")?,
            env_account("spare", "claude", "SPARE_KEY"),
            env_account("local", "muse", "MUSE_KEY"),
        ]
        .join("\n");
        let mut harness = started(&config, &world).await?;
        harness.env.push(("SPARE_KEY".to_owned(), "SECRET_TOKEN_SPARE".to_owned()));
        let server = world.usage.get().await?;
        server.serve(Provider::Codex, Method::Get, WHAM, codex_route(WORK, "acct-work"));
        server.serve(Provider::Codex, Method::Get, WHAM, codex_route(CACHED, "acct-cached"));
        server.serve(Provider::Claude, Method::Get, "/api/oauth/usage", Route::status(401));
        harness.run(&["usage"]).await?.ok()?;
        world.clock.advance(Duration::from_secs(600));
        server.replace(Provider::Codex, Method::Get, WHAM, codex_route(WORK, "acct-work"));
        server.serve(
            Provider::Codex,
            Method::Get,
            WHAM,
            Route::status(500).requires_header("chatgpt-account-id", "acct-cached"),
        );
        let ran = harness.run(&["usage"]).await?;
        ran.ok()?;
        let expected = [
            "cached\tcodex\t5-hour\t80% used\t20% left\tresets 2026-01-01T05:00:00Z (in 4h50m)\tstale, fetched 2026-01-01T00:00:00Z",
            "local\tmuse\tunavailable\tnot supported for this provider",
            "spare\tclaude\tunavailable\tlogin expired or rejected",
            "work\tcodex\t5-hour\t42% used\t58% left\tresets 2026-01-01T02:00:00Z (in 1h50m)",
            "work\tcodex\tweekly\t12% used\t88% left\tresets 2028-02-29T23:59:59Z (in 789d23h)",
            "work\tcodex\tcredits\t-\t12.40 credits left\t-",
        ]
        .join("\n");
        assert_eq!(ran.stdout, format!("{expected}\n"));
        assert!(ran.stderr.is_empty(), "{}", ran.stderr);
        assert!(!ran.stdout.contains('\u{1b}'));
        assert!(ran.stdout.lines().all(|line| !line.ends_with(' ')));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn usage_needs_no_gateway_while_ls_fails_without_one() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let config = login_account(&world, "work", "acct-work")?;
        let harness = started(&config, &world).await?;
        let server = world.usage.get().await?;
        server.serve(
            Provider::Codex,
            Method::Get,
            WHAM,
            codex_route(ONE, "acct-work"),
        );
        harness.run(&["usage"]).await?.ok()?;
        let ls = harness.run(&["ls"]).await?;
        assert_eq!(ls.exit, ExitClass::Failure, "{}", ls.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn usage_shows_decimals_money_left_and_a_dash_for_no_reset() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let config = [
            env_account("router", "openrouter", "ROUTER_KEY"),
            env_account("gem", "gemini", "GEM_KEY"),
        ]
        .join("\n");
        let mut harness = started(&config, &world).await?;
        harness
            .env
            .push(("ROUTER_KEY".to_owned(), "SECRET_TOKEN_ROUTER".to_owned()));
        harness
            .env
            .push(("GEM_KEY".to_owned(), "AIzaSECRET_TOKEN_GEM".to_owned()));
        let server = world.usage.get().await?;
        let bearer = "Bearer SECRET_TOKEN_ROUTER";
        server.serve(
            Provider::Openrouter,
            Method::Get,
            "/api/v1/key",
            Route::json(200, KEY).requires_header("authorization", bearer),
        );
        server.serve(
            Provider::Openrouter,
            Method::Get,
            "/api/v1/credits",
            Route::status(403).requires_header("authorization", bearer),
        );
        let ran = harness.run(&["usage"]).await?;
        ran.ok()?;
        assert_eq!(
            ran.stdout,
            "gem\tgemini\tunavailable\tnot supported: Gemini API keys have no quota method\nrouter\topenrouter\tKey limit\t24.68% used\t37.66 USD left\t-\n"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn usage_json_prints_the_report_compactly_and_unchanged() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let config = login_account(&world, "work", "acct-work")?;
        let harness = started(&config, &world).await?;
        let server = world.usage.get().await?;
        server.serve(Provider::Codex, Method::Get, WHAM, codex_route(ONE, "acct-work"));
        let ran = harness.run(&["usage", "--json"]).await?;
        ran.ok()?;
        let expected = "{\"schema\":1,\"generated_at\":1767225600,\"accounts\":[{\"account\":\"work\",\"provider\":\"codex\",\"freshness\":{\"state\":\"fresh\"},\"outcome\":{\"state\":\"windows\",\"windows\":[{\"kind\":\"five_hour\",\"label\":\"5-hour\",\"used_percent\":42.0,\"remaining\":{\"kind\":\"percent\",\"value\":58.0},\"resets_at\":1767232800}]}}]}\n";
        assert_eq!(ran.stdout, expected);
        assert!(ran.stderr.is_empty(), "{}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn usage_without_accounts_prints_nothing_and_an_empty_json_report() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = started("", &world).await?;
        let text = harness.run(&["usage"]).await?;
        text.ok()?;
        assert!(text.stdout.is_empty(), "{}", text.stdout);
        let json = harness.run(&["usage", "--json"]).await?;
        json.ok()?;
        assert_eq!(
            json.stdout,
            "{\"schema\":1,\"generated_at\":1767225600,\"accounts\":[]}\n"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_usage_run_exits_cancelled_with_no_output() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let config = login_account(&world, "work", "acct-work")?;
        let harness = started(&config, &world).await?;
        let token = CancellationToken::new();
        token.cancel();
        let ran = harness.command(&["usage"]).cancel(token).run().await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stdout.is_empty(), "{}", ran.stdout);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unusable_cache_location_fails_the_command_and_names_the_path() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let config = login_account(&world, "work", "acct-work")?;
        let harness = started(&config, &world).await?;
        let server = world.usage.get().await?;
        server.serve(
            Provider::Codex,
            Method::Get,
            WHAM,
            codex_route(ONE, "acct-work"),
        );
        std::fs::create_dir_all(world.cache_dir().as_path())?;
        std::fs::write(
            world.cache_dir().as_path().join("usage"),
            "a file, not a directory",
        )?;
        let ran = harness.run(&["usage"]).await?;
        assert_eq!(ran.exit, ExitClass::Failure, "{}", ran.stderr);
        assert!(ran.stdout.is_empty(), "{}", ran.stdout);
        assert!(ran.stderr.contains("usage.json"), "{}", ran.stderr);
        assert!(!ran.stderr.contains("SECRET_TOKEN_CODEX"), "{}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn usage_reads_the_cache_inside_the_ttl_and_fetches_again_after_it() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let config = login_account(&world, "work", "acct-work")?;
        let harness = started(&config, &world).await?;
        let server = world.usage.get().await?;
        server.serve(
            Provider::Codex,
            Method::Get,
            WHAM,
            codex_route(ONE, "acct-work"),
        );
        let first = harness.run(&["usage"]).await?;
        first.ok()?;
        assert!(first.stdout.contains("42% used"), "{}", first.stdout);
        server.replace(
            Provider::Codex,
            Method::Get,
            WHAM,
            codex_route(CACHED, "acct-work"),
        );
        world.clock.advance(Duration::from_secs(60));
        let inside = harness.run(&["usage"]).await?;
        inside.ok()?;
        assert!(inside.stdout.contains("42% used"), "{}", inside.stdout);
        assert!(!inside.stdout.contains("80% used"), "{}", inside.stdout);
        world.clock.advance(Duration::from_secs(70));
        let after = harness.run(&["usage"]).await?;
        after.ok()?;
        assert!(after.stdout.contains("80% used"), "{}", after.stdout);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_harness_that_sets_no_usage_endpoints_points_every_provider_at_a_closed_port()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        let text = format!("{:?}", harness.usage_endpoints);
        assert_eq!(
            text.matches(CLOSED_ORIGIN).count(),
            Provider::ALL.len(),
            "{text}"
        );
        Ok(())
    })
    .await
}

fn make_unusable(path: &std::path::Path, mode: u32) -> Result<(), Failure> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

async fn failing_store(read: bool) -> TestResult {
    let world = World::builder().build()?;
    let config = login_account(&world, "work", "acct-work")?;
    let harness = started(&config, &world).await?;
    let server = world.usage.get().await?;
    server.serve(
        Provider::Codex,
        Method::Get,
        WHAM,
        codex_route(ONE, "acct-work"),
    );
    let state = world.cache_dir().as_path().join("usage");
    std::fs::create_dir_all(&state)?;
    let file = state.join("usage.json");
    if read {
        std::fs::write(&file, "{}")?;
        make_unusable(&file, 0o000)?;
    } else {
        make_unusable(&state, 0o555)?;
    }
    let ran = harness.run(&["usage"]).await?;
    make_unusable(&state, 0o755)?;
    if read {
        make_unusable(&file, 0o644)?;
    }
    assert_eq!(ran.exit, ExitClass::Failure, "{}", ran.stderr);
    assert!(ran.stdout.is_empty(), "{}", ran.stdout);
    assert!(ran.stderr.contains("usage.json"), "{}", ran.stderr);
    Ok(())
}

#[tokio::test]
async fn a_cache_file_that_cannot_be_read_fails_the_command() -> TestResult {
    guard(failing_store(true)).await
}

#[tokio::test]
async fn a_cache_directory_that_cannot_be_written_fails_the_command() -> TestResult {
    guard(failing_store(false)).await
}
