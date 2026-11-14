//! The CSRF triple: whether code this app served made a write, and whether its page keeps a
//! write contract the box takes.

use suzatary_base::refusal::{Refusal, shown};

/// The app's own header, which forces a CORS preflight this server never answers.
pub const ACT_HEADER: &str = "x-suzatary-act";

/// What a request carries that the CSRF triple reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActHeaders<'a> {
    /// `X-Suzatary-Act`, absent as `None`.
    pub act: Option<&'a str>,
    /// `X-Suzatary-Client`, the write contract the page's build keeps, absent as `None`.
    pub client: Option<&'a str>,
    /// `Content-Type`, absent as `None`.
    pub content_type: Option<&'a str>,
    /// `Sec-Fetch-Site`, absent as `None`, which passes because non-browsers do not send it.
    pub site: Option<&'a str>,
}

/// The CSRF triple: did code this app served make this request.
pub fn check_csrf(headers: &ActHeaders<'_>) -> Result<(), Refusal> {
    if headers.act != Some("1") {
        return Err(Refusal::forbidden(format!(
            "a write must carry {ACT_HEADER}: 1"
        )));
    }
    // Parameters such as `charset` do not change the content type.
    let kind = headers
        .content_type
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if kind != "application/json" {
        return Err(Refusal::forbidden(
            "a write must be Content-Type: application/json",
        ));
    }
    match headers.site {
        None => Ok(()),
        Some("same-origin") => current(headers.client),
        Some(labelled) => Err(Refusal::forbidden(format!(
            "a write must be same-origin; the browser called this {}",
            shown(labelled)
        ))),
    }
}

/// The header a page's build names its write contract in.
pub const CLIENT_HEADER: &str = "x-suzatary-client";

/// The oldest write contract the box takes from a page; an older page is refused before it writes
/// and reloads. Raise it in the same series as a change to what a page writes.
pub const CLIENT_MIN: u32 = 12;

/// The contract this build's own page writes by: what a test writes as.
pub const CLIENT: &str = "12";

/// A page keeps [`CLIENT_MIN`] or later.
fn current(client: Option<&str>) -> Result<(), Refusal> {
    match client.and_then(|said| said.trim().parse::<u32>().ok()) {
        Some(contract) if contract >= CLIENT_MIN => Ok(()),
        said => Err(Refusal::new(
            suzatary_base::refusal::Status::UpgradeRequired,
            format!(
                "this page writes by contract {}, and the box takes {CLIENT_MIN} or later, so this act was not sent: the page says Suzatary <version> is ready with Reload, and it can be sent again after Reload",
                said.map_or_else(|| "none".to_owned(), |n| n.to_string())
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The build's own page writes by a contract its box takes.
    #[test]
    fn this_builds_page_keeps_the_contract_its_box_takes() {
        assert_eq!(current(Some(CLIENT)), Ok(()));
        assert_eq!(CLIENT.parse::<u32>().ok(), Some(CLIENT_MIN));
    }
    #[test]
    fn the_csrf_triple_refuses_each_of_its_three_ways() {
        let good = ActHeaders {
            act: Some("1"),
            client: Some(CLIENT),
            content_type: Some("application/json"),
            site: Some("same-origin"),
        };
        assert_eq!(check_csrf(&good), Ok(()));

        let no_header = ActHeaders {
            act: None,
            ..good.clone()
        };
        assert_eq!(
            check_csrf(&no_header).unwrap_err().message(),
            "a write must carry x-suzatary-act: 1"
        );

        let wrong_type = ActHeaders {
            content_type: Some("text/plain"),
            ..good.clone()
        };
        assert_eq!(
            check_csrf(&wrong_type).unwrap_err().message(),
            "a write must be Content-Type: application/json"
        );

        let cross = ActHeaders {
            site: Some("cross-site"),
            ..good.clone()
        };
        assert_eq!(
            check_csrf(&cross).unwrap_err().message(),
            "a write must be same-origin; the browser called this 'cross-site'"
        );
    }
    /// Content-type parameters are ignored and a missing `Sec-Fetch-Site` passes.
    #[test]
    fn a_charset_is_still_json_and_an_absent_site_label_passes() {
        assert_eq!(
            check_csrf(&ActHeaders {
                act: Some("1"),
                client: None,
                content_type: Some("Application/JSON; charset=utf-8"),
                site: None,
            }),
            Ok(())
        );
    }
    /// A page older than the box's minimum is told to upgrade; a client that is not a page is not asked.
    #[test]
    fn a_page_writing_by_an_older_contract_is_told_to_load_the_new_one() {
        let page = |client| ActHeaders {
            act: Some("1"),
            client,
            content_type: Some("application/json"),
            site: Some("same-origin"),
        };
        for stale in [
            None,
            Some("1"),
            Some("2"),
            Some("3"),
            Some("4"),
            Some("5"),
            Some("6"),
            Some("7"),
            Some("8"),
            Some("9"),
            Some("10"),
            Some("11"),
            Some("not a number"),
        ] {
            let refused = check_csrf(&page(stale)).unwrap_err();
            assert_eq!(refused.status().code(), 426, "{stale:?}");
        }
        assert_eq!(
            check_csrf(&page(None)).unwrap_err().message(),
            "this page writes by contract none, and the box takes 12 or later, so this act was not sent: the page says Suzatary <version> is ready with Reload, and it can be sent again after Reload"
        );
        assert_eq!(check_csrf(&page(Some("12"))), Ok(()));
        assert_eq!(check_csrf(&page(Some("13"))), Ok(()));
        assert_eq!(
            check_csrf(&ActHeaders {
                site: None,
                ..page(None)
            }),
            Ok(()),
            "the Android shell's own registration carries no bundle"
        );
    }
}
