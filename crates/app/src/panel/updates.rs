//! Settings' "Check for updates" (ui/about.rs): core's latest_release on a
//! press of the button, never by itself; the answer goes into [`Ui`].

use super::view::Ui;
use super::{Panel, spawn};
use blitznir_core::llm::release::newer;
use std::sync::Arc;

/// Where releases are published.
pub const REPO: &str = "Thoralf-M/blitznir";

/// What the last check said.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Updates {
    #[default]
    Unasked,
    Checking,
    Latest,
    Newer {
        version: String,
        url: String,
    },
    Failed(String),
}

impl Panel {
    pub fn check_updates(self: &Arc<Self>) {
        self.with_ui(|ui: &mut Ui| ui.updates = Updates::Checking);
        let me = self.clone();
        let ask = move || {
            let got = me.block_on(me.client.latest_release(REPO));
            let now = match got {
                Ok(r) if newer(env!("CARGO_PKG_VERSION"), &r.version) => Updates::Newer {
                    version: r.version,
                    url: r.url,
                },
                Ok(_) => Updates::Latest,
                Err(e) => Updates::Failed(e.to_string()),
            };
            log::info!("update check: {now:?}");
            me.with_ui(|ui| ui.updates = now);
        };
        if let Err(e) = spawn("update-check", ask) {
            self.with_ui(|ui| ui.updates = Updates::Failed(e));
        }
    }
}
