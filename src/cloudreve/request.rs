use crate::cloudreve::CloudreveClient;
use reqwest;

impl CloudreveClient {
    pub(crate) async fn request_builder(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut builder = self.client.request(method, &url);

        let state = self.state.read().await;
        if !state.token.is_empty() {
            builder = builder.header("Authorization", format!("Bearer {}", state.token));
        }
        builder
    }
}
