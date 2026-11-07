//! ntfy: one POST, in the clear, so a push is a [`Printed`] the core composed.

use suzatary_core::capability::Printed;

use crate::http::{Http, Req};

/// ntfy's own service, used when no `base` is configured.
pub const DEFAULT_BASE: &str = "https://ntfy.sh";

/// One ntfy POST; `Err` names an unreachable relay or a status not 2xx.
pub(super) fn send(http: &dyn Http, base: &str, topic: &str, push: &Printed) -> Result<(), String> {
    let req = Req::post(format!("{base}/{topic}"), push.body.as_bytes())
        .header("Title", push.title)
        .header("Click", &push.click)
        .header("Tags", push.tag)
        .header("Content-Type", "text/plain; charset=utf-8");
    match http.send(req) {
        Ok(resp) if resp.ok() => Ok(()),
        Ok(resp) => Err(format!("ntfy answered {}", resp.status)),
        Err(failed) => Err(format!("ntfy: {failed}")),
    }
}
