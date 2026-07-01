#[cfg(test)]
use super::*;

fn status_check(status: &str, conclusion: &str, details_url: &str) -> GhStatusCheck {
    GhStatusCheck {
        status: status.to_owned(),
        conclusion: conclusion.to_owned(),
        details_url: details_url.to_owned(),
    }
}

#[test]
fn best_check_url_prefers_failed_links() {
    let checks = [
        GhCheck {
            bucket: "pass".to_owned(),
            link: "https://github.com/pass".to_owned(),
        },
        GhCheck {
            bucket: "fail".to_owned(),
            link: "https://github.com/fail".to_owned(),
        },
        GhCheck {
            bucket: "pending".to_owned(),
            link: "https://github.com/pending".to_owned(),
        },
    ];

    assert_eq!(
        best_check_url(&checks).as_deref(),
        Some("https://github.com/fail")
    );
}

#[test]
fn best_status_check_url_prefers_failed_then_pending_then_success() {
    let checks = [
        status_check("COMPLETED", "SUCCESS", "https://github.com/success"),
        status_check("IN_PROGRESS", "", "https://github.com/pending"),
        status_check("COMPLETED", "FAILURE", "https://github.com/failure"),
    ];

    assert_eq!(
        best_status_check_url(&checks).as_deref(),
        Some("https://github.com/failure")
    );

    let checks = [
        status_check("COMPLETED", "SUCCESS", "https://github.com/success"),
        status_check("IN_PROGRESS", "", "https://github.com/pending"),
    ];

    assert_eq!(
        best_status_check_url(&checks).as_deref(),
        Some("https://github.com/pending")
    );
}

#[test]
fn pr_checks_tab_url_appends_checks_to_http_pr_url() {
    assert_eq!(
        pr_checks_tab_url("https://github.com/jackin-project/jackin/pull/565").as_deref(),
        Some("https://github.com/jackin-project/jackin/pull/565/checks")
    );
    assert!(pr_checks_tab_url("file:///tmp/pr").is_none());
}
}
