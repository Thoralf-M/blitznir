//! Settings' "Check for updates": the newest release on GitHub. Asked only
//! when the button is pressed: local first, nothing goes out by itself.

use super::Client;
use crate::{Error, Result};
use serde_json::Value;
use std::time::Duration;

/// A published release: its version (the tag without "v") and its page.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
}

impl Client {
    /// The latest release of `repo` ("owner/name") on GitHub.
    pub async fn latest_release(&self, repo: &str) -> Result<Release> {
        let url = format!("https://api.github.com/repos/{repo}/releases/latest");
        self.release_at(&url).await
    }

    async fn release_at(&self, url: &str) -> Result<Release> {
        let resp = (self.http.get(url))
            .header("User-Agent", "blitznir") // GitHub refuses requests without one
            .header("Accept", "application/vnd.github+json")
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        // what GitHub answers for a repo without a release
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::Llm("no release has been published yet".into()));
        }
        if !resp.status().is_success() {
            return Err(Error::Llm(format!("GitHub answered {}", resp.status())));
        }
        let data: Value = resp.json().await?;
        let tag = data["tag_name"]
            .as_str()
            .ok_or_else(|| Error::Llm("no tag_name".into()))?;
        Ok(Release {
            version: tag.trim_start_matches('v').to_string(),
            url: data["html_url"].as_str().unwrap_or_default().to_string(),
        })
    }
}

/// `version` ("0.2.0", "v0.10.1") is newer than `current`: their numbers
/// compared in order, a missing one as 0; anything after a "-" is ignored.
pub fn newer(current: &str, version: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        let v = v.trim_start_matches('v');
        let v = v.split('-').next().unwrap_or_default();
        v.split('.').map(|n| n.parse().unwrap_or(0)).collect()
    };
    let (a, b) = (parts(current), parts(version));
    let n = a.len().max(b.len());
    let at = |p: &[u64], i: usize| p.get(i).copied().unwrap_or(0);
    (0..n)
        .map(|i| at(&b, i).cmp(&at(&a, i)))
        .find(|o| o.is_ne())
        .is_some_and(|o| o.is_gt())
}

#[cfg(test)]
mod tests {
    use super::newer;
    use crate::llm::Client;
    use crate::llm::mock::{Reply, mock};

    #[tokio::test]
    async fn no_release_yet_says_so_and_a_release_is_read() {
        let m = mock(vec![Reply::Plain(404, "{}".into())], "");
        let err = Client::new().unwrap().release_at(&m.ep.url).await;
        assert_eq!(
            err.unwrap_err().to_string(),
            "no release has been published yet"
        );
        let body = r#"{"tag_name": "v0.2.0", "html_url": "https://example.org/r"}"#;
        let m = mock(vec![Reply::Plain(200, body.into())], "");
        let r = Client::new().unwrap().release_at(&m.ep.url).await.unwrap();
        assert_eq!(
            (r.version.as_str(), r.url.as_str()),
            ("0.2.0", "https://example.org/r")
        );
    }

    #[test]
    fn a_release_is_newer_by_its_numbers_not_its_text() {
        assert!(newer("0.1.0", "v0.2.0"));
        assert!(newer("0.9.0", "0.10.0"), "10 > 9, unlike as text");
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("0.2.0", "0.1.9"));
        assert!(newer("0.1", "0.1.1"), "a missing part is 0");
        assert!(!newer("1.0.0", "1.0.0-beta"));
    }
}
