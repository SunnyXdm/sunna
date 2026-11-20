use super::Sharing;

pub async fn status() -> Sharing {
    Sharing {
        state: "off".into(),
        reason: Some("This build can only connect to other computers.".into()),
        ..Sharing::default()
    }
}

pub async fn set(_: bool, _: &(dyn Fn(Sharing) + Sync)) -> Sharing {
    status().await
}

pub fn open_permission(_: &str) -> Result<(), String> {
    Err("Permission settings are unavailable in this build.".into())
}
