use url::Url;

pub struct CommandExecutorConfig {
    pub home_assistant_base_url: Url,
    pub home_assistant_token: String,
    pub weather_latitude: Option<f64>,
    pub weather_longitude: Option<f64>,
}

impl CommandExecutorConfig {
    pub fn new(
        home_assistant_base_url: Url,
        home_assistant_token: String,
        weather_latitude: Option<f64>,
        weather_longitude: Option<f64>,
    ) -> Self {
        Self {
            home_assistant_base_url,
            home_assistant_token,
            weather_latitude,
            weather_longitude,
        }
    }
}
