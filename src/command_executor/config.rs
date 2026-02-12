use url::Url;

pub struct CommandExecutorConfig {
    // TODO: use Url
    pub home_assistant_base_url: Url,
    // TODO: use Secret
    pub home_assistant_token: String,
}

impl CommandExecutorConfig {
    pub fn new(home_assistant_base_url: Url, home_assistant_token: String) -> Self {
        Self {
            home_assistant_base_url,
            home_assistant_token,
        }
    }
}
