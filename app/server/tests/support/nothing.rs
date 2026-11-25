use suzatary_base::capability::Nothing;
use suzatary_secretary::secretary::Secretary;

use crate::server::Server;

impl Server<'static> {
    /// The server's record, every field inert.
    #[must_use]
    pub fn nothing() -> Self {
        Server {
            secretary: Secretary::nothing(),
            whois: &Nothing,
            notify: &Nothing,
            answers: &Nothing,
            devices: &Nothing,
            models: &Nothing,
        }
    }
}
