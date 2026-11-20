//! What this machine is (OS, laptop or desktop), told to launchers that
//! probe with the right token so they can draw it properly.

use sunna_proto::messages::HostAbout;

pub fn detect() -> HostAbout {
    #[cfg(target_os = "macos")]
    let about = macos();
    #[cfg(target_os = "linux")]
    let about = linux();
    #[cfg(target_os = "windows")]
    let about = windows();
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
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
    } else if model == "Mac mini" || model == "Mac Studio" {
        "mini"
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
#[cfg(any(target_os = "linux", target_os = "windows", test))]
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
        3 | 4 | 5 | 6 | 7 | 13 | 15 | 16 | 24 => "desktop",
        35 | 36 => "mini",
        8 | 9 | 10 | 11 | 14 | 30 | 31 | 32 => "laptop",
        17 | 23 | 25 | 28 | 29 => "server",
        _ => "",
    }
}

#[cfg(target_os = "windows")]
fn windows() -> HostAbout {
    use windows::Win32::System::Power::{GetPwrCapabilities, SYSTEM_POWER_CAPABILITIES};

    const VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    const BIOS: &str = r"HARDWARE\DESCRIPTION\System\BIOS";
    let os = windows_version(
        &registry_string(VERSION, "ProductName"),
        &registry_string(VERSION, "CurrentBuildNumber"),
        &registry_string(VERSION, "DisplayVersion"),
    );
    let vendor = registry_string(BIOS, "SystemManufacturer");
    let product = registry_string(BIOS, "SystemProductName");
    let board = registry_string(BIOS, "BaseBoardProduct");
    let mut power = SYSTEM_POWER_CAPABILITIES::default();
    // SAFETY: The output has the exact type and size the API expects.
    let lid = unsafe { GetPwrCapabilities(&mut power) }.then_some(power.LidPresent);
    let (model, mut device) = windows_hardware(&vendor, &product, &board, lid);
    if device != "vm" {
        // Win32_SystemEnclosure exposes SMBIOS chassis types (including Mini PC).
        if let Ok(output) = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "(Get-CimInstance Win32_SystemEnclosure).ChassisTypes -join ','"])
            .output()
        {
            if output.status.success() && String::from_utf8_lossy(&output.stdout)
                .trim().split(',').any(|kind| matches!(kind.trim(), "35" | "36"))
            {
                device = "mini";
            }
        }
    }
    if !matches!(device, "vm" | "mini") && os.contains("Windows Server") {
        device = "server";
    }
    HostAbout {
        os,
        device: device.into(),
        model,
        width: 0,
        height: 0,
    }
}

#[cfg(target_os = "windows")]
fn registry_string(path: &str, name: &str) -> String {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY,
    };

    let path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let flags = RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY;
    let mut bytes = 0;
    // SAFETY: Strings are NUL terminated and live through both calls. This
    // uses the predefined HKLM handle; RegGetValueW owns opening the subkey.
    let size = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(path.as_ptr()),
            PCWSTR(name.as_ptr()),
            flags,
            None,
            None,
            Some(&mut bytes),
        )
    };
    if size.is_err() || bytes == 0 || bytes > 65536 || bytes % 2 != 0 {
        return String::new();
    }
    let mut value = vec![0u16; bytes as usize / 2];
    // SAFETY: `bytes` is the buffer's byte capacity. REG_SZ restricts the
    // output to UTF-16 text; a growing value returns an error, not a partial name.
    let read = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(path.as_ptr()),
            PCWSTR(name.as_ptr()),
            flags,
            None,
            Some(value.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    if read.is_err() || bytes % 2 != 0 {
        return String::new();
    }
    let value = &value[..bytes as usize / 2];
    let end = value.iter().position(|&c| c == 0).unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end]).trim().to_string()
}

/// ProductName can still say Windows 10 on Windows 11; preserve its edition.
#[cfg(any(target_os = "windows", test))]
fn windows_version(product: &str, build: &str, display: &str) -> String {
    let product = product.trim();
    let windows_11 = build
        .trim()
        .parse::<u32>()
        .is_ok_and(|build| build >= 22000);
    let name = if windows_11 && product.starts_with("Windows 10") {
        product.replacen("Windows 10", "Windows 11", 1)
    } else if product.is_empty() {
        if windows_11 { "Windows 11" } else { "Windows" }.to_string()
    } else {
        product.to_string()
    };
    if display.trim().is_empty() {
        name
    } else {
        format!("{name} {}", display.trim())
    }
}

#[cfg(any(target_os = "windows", test))]
fn windows_hardware(
    vendor: &str,
    product: &str,
    board: &str,
    lid: Option<bool>,
) -> (String, &'static str) {
    let useful = |value: &str| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "system manufacturer"
                | "system product name"
                | "default string"
                | "to be filled by o.e.m."
                | "to be filled by oem"
                | "not applicable"
                | "none"
                | "unknown"
        )
    };
    let vendor = if useful(vendor) { vendor.trim() } else { "" };
    let product = if useful(product) {
        product.trim()
    } else if useful(board) {
        board.trim()
    } else {
        ""
    };
    let model = if vendor.is_empty()
        || product
            .to_ascii_lowercase()
            .starts_with(&vendor.to_ascii_lowercase())
    {
        product.to_string()
    } else if product.is_empty() {
        vendor.to_string()
    } else {
        format!("{vendor} {product}")
    };
    // Reuse the DMI hypervisor names. A lid distinguishes a laptop from a
    // desktop without mistaking a desktop UPS for a laptop battery.
    let chassis = match lid {
        Some(true) => "9",
        Some(false) => "3",
        None => "",
    };
    (model, linux_device(vendor, product, chassis))
}

#[cfg(test)]
mod windows_tests {
    use super::{windows_hardware, windows_version};

    #[test]
    fn windows_version_preserves_editions_and_server_names() {
        assert_eq!(
            windows_version("Windows 10 Pro", "26200", "25H2"),
            "Windows 11 Pro 25H2"
        );
        assert_eq!(
            windows_version("Windows 10 Home", "21999", "21H2"),
            "Windows 10 Home 21H2"
        );
        assert_eq!(
            windows_version("Windows 10 Pro", "22000", ""),
            "Windows 11 Pro"
        );
        assert_eq!(
            windows_version("Windows 11 Pro", "26200", "25H2"),
            "Windows 11 Pro 25H2"
        );
        assert_eq!(
            windows_version("Windows Server 2025 Standard", "26100", "24H2"),
            "Windows Server 2025 Standard 24H2"
        );
        assert_eq!(windows_version("", "", ""), "Windows");
        assert_eq!(windows_version("", "22000", ""), "Windows 11");
        assert_eq!(
            windows_version("Windows 10 Pro", "invalid", ""),
            "Windows 10 Pro"
        );
    }

    #[test]
    fn windows_hardware_uses_bios_names_and_lid_with_vm_precedence() {
        assert_eq!(
            windows_hardware("LENOVO", "ThinkPad T14", "board", Some(true)),
            ("LENOVO ThinkPad T14".into(), "laptop")
        );
        assert_eq!(
            windows_hardware("ASUS", "System Product Name", "PRIME B550", Some(false)),
            ("ASUS PRIME B550".into(), "desktop")
        );
        assert_eq!(
            windows_hardware("VMware, Inc.", "VMware Virtual Platform", "", Some(true)).1,
            "vm"
        );
        assert_eq!(
            windows_hardware("Microsoft Corporation", "Virtual Machine", "", Some(false)).1,
            "vm"
        );
        assert_eq!(
            windows_hardware("Dell", "Dell XPS", "", Some(true)).0,
            "Dell XPS"
        );
        assert_eq!(
            windows_hardware("To Be Filled By O.E.M.", "Default string", "", None),
            (String::new(), "")
        );
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

#[cfg(test)]
mod device_kind_tests {
    use super::linux_device;
    #[test]
    fn mini_chassis_and_hypervisor_precedence() {
        assert_eq!(linux_device("", "", "35"), "mini");
        assert_eq!(linux_device("", "", "36"), "mini");
        assert_eq!(linux_device("QEMU", "", "35"), "vm");
        assert_eq!(linux_device("", "", "3"), "desktop");
        assert_eq!(linux_device("", "", "9"), "laptop");
        assert_eq!(linux_device("", "", "23"), "server");
    }
}
