// CreateServiceW persists arguments as part of ImagePath.
pub fn validate_service_args(args: &[std::ffi::OsString]) -> Result<(), &'static str> {
    for arg in args {
        let arg = arg.to_string_lossy();
        if arg == "--token" || arg.starts_with("--token=") {
            return Err("The service reads its sharing key from host.key. Remove --token.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checks_both_cli_spellings_including_empty_values() {
        for arg in ["--token", "--token=fixture", "--token="] {
            assert!(validate_service_args(&[arg.into()]).is_err());
        }
        assert!(validate_service_args(&["--source".into(), "screen".into()]).is_ok());
    }
}
