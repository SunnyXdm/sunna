//! What this machine is (OS, laptop or desktop), told to launchers that
//! probe with the right token so they can draw it properly.

use sunna_proto::messages::HostAbout;

pub fn detect() -> HostAbout {
    #[cfg(target_os = "macos")]
    let about = macos();
    #[cfg(target_os = "linux")]
    let about = linux();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let about = HostAbout::default();
    tracing::info!(os = %about.os, device = %about.device, model = %about.model, "this machine");
    about
}

#[cfg(target_os = "macos")]
fn macos() -> HostAbout {
    let run = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
            .unwrap_or_default()
    };
    let version = run("/usr/bin/sw_vers", &["-productVersion"]);
    let os = match version.trim() {
        "" => "macOS".to_string(),
        version => format!("macOS {version}"),
    };
    // "MacBook Air", "Mac mini"... The model identifier ("Mac14,2") doesn't
    // say which. `mini` leaves out serial numbers.
    let hardware = run(
        "/usr/sbin/system_profiler",
        &["SPHardwareDataType", "-detailLevel", "mini"],
    );
    let model = hardware
        .lines()
        .find_map(|line| line.trim().strip_prefix("Model Name:"))
        .map(|name| name.trim().to_string())
        .unwrap_or_default();
    let device = if model.starts_with("MacBook") {
        "laptop"
    } else if model.contains("Virtual") {
        "vm"
    } else if model.is_empty() {
        ""
    } else {
        "desktop"
    };
    HostAbout {
        os,
        device: device.into(),
        model,
        width: 0,
        height: 0,
    }
}

#[cfg(target_os = "linux")]
fn linux() -> HostAbout {
    let read = |path: &str| {
        std::fs::read_to_string(path)
            .map(|text| text.trim().to_string())
            .unwrap_or_default()
    };
    let release = read("/etc/os-release");
    let field = |key: &str| {
        release.lines().find_map(|line| {
            line.strip_prefix(key)
                .and_then(|rest| rest.strip_prefix('='))
                .map(|value| value.trim_matches('"').to_string())
        })
    };
    let os = field("PRETTY_NAME")
        .or_else(|| field("NAME"))
        .unwrap_or_else(|| "Linux".into());
    let vendor = read("/sys/class/dmi/id/sys_vendor");
    let product = read("/sys/class/dmi/id/product_name");
    HostAbout {
        os,
        device: linux_device(&vendor, &product, &read("/sys/class/dmi/id/chassis_type")).into(),
        model: String::new(),
        width: 0,
        height: 0,
    }
}

/// From DMI: hypervisors first (they report odd chassis types), then the
/// SMBIOS chassis type.
#[cfg(target_os = "linux")]
fn linux_device(vendor: &str, product: &str, chassis: &str) -> &'static str {
    const HYPERVISORS: [&str; 10] = [
        "QEMU",
        "KVM",
        "VMware",
        "VirtualBox",
        "innotek",
        "Xen",
        "Google",
        "Amazon EC2",
        "Parallels",
        "DigitalOcean",
    ];
    if HYPERVISORS
        .iter()
        .any(|name| vendor.contains(name) || product.contains(name))
        || product.contains("Virtual Machine")
    {
        return "vm";
    }
    match chassis.parse::<u32>().unwrap_or(0) {
        3 | 4 | 5 | 6 | 7 | 13 | 15 | 16 | 24 | 35 | 36 => "desktop",
        8 | 9 | 10 | 11 | 14 | 30 | 31 | 32 => "laptop",
        17 | 23 | 25 | 28 | 29 => "server",
        _ => "",
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::linux_device;

    #[test]
    fn devices_from_dmi() {
        assert_eq!(
            linux_device("Micro-Star International Co., Ltd.", "MS-7C56", "3"),
            "desktop"
        );
        assert_eq!(linux_device("LENOVO", "20XW0026US", "10"), "laptop");
        assert_eq!(linux_device("Google", "Google Compute Engine", "1"), "vm");
        assert_eq!(
            linux_device("QEMU", "Standard PC (Q35 + ICH9, 2009)", "1"),
            "vm"
        );
        assert_eq!(linux_device("", "", ""), "");
    }
}
