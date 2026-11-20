# Sunna Display

Original C++17 IddCx/UMDF 2 virtual display driver, built outside Cargo. `sunnad`
creates a software device with hardware ID `SunnaDisplay` using `SwDeviceCreate`
and keeps its handle open for the daemon lifetime. There are no monitors until
an ADD IOCTL succeeds. Closing the software-device handle removes the device.
The INF also supports `Root\SunnaDisplay` for manual testing.

The frame thread consumes and acknowledges buffers immediately on Windows' selected
render adapter; Sunna captures separately through Desktop Duplication. Hardware
cursors (alpha/masked colour, up to 256x256) are drained separately for viewer-side
rendering. Actual 60–240 Hz cadence needs an installed-driver test with animation.

## Control interface

[include/sunna_display.h](include/sunna_display.h) is the plain-C ABI source of
truth. Interface GUID: **`{27e046a1-9ba8-4479-963f-cd0f642788c1}`**. Enumerate it
with SetupAPI/Configuration Manager, open with `GENERIC_WRITE`, and use
`DeviceIoControl`. All fields are little-endian `uint32_t`, naturally aligned;
there are no pointers, packing directives, or architecture-dependent fields.
All IOCTLs are `FILE_DEVICE_UNKNOWN`, `METHOD_BUFFERED`, `FILE_WRITE_ACCESS`.

| IOCTL | Code | Input | Output |
|---|---|---|---|
| INFO (0x800) | 0x0022a000 | none | `{version=1, max_monitors=4}` (8 bytes) |
| ADD (0x801) | 0x0022a004 | `{version=1, width, height, refresh_hz}` (16 bytes) | `{id}` (4 bytes) |
| REMOVE (0x802) | 0x0022a008 | `{id}` (4 bytes) | none |

Input lengths must match exactly; output buffers must have at least the stated
size. ADD requires even dimensions, width 640–7680, height 480–4320 and 24–500 Hz.
`STATUS_DEVICE_NOT_READY` means retry after adapter initialization/power-up;
invalid values return `STATUS_INVALID_PARAMETER`, wrong input lengths return
`STATUS_INVALID_BUFFER_SIZE`, and short output buffers fail before side effects.
Four occupied slots return `STATUS_INSUFFICIENT_RESOURCES`; unknown REMOVE IDs
return `STATUS_NOT_FOUND`. INFO remains available during initialization.

IDs are opaque, nonzero and never reused within a driver-host lifetime. EDID serial
and ContainerId are deterministic per ID; allocation order restarts with the driver
host. Retain successful ADD results. There is no per-control-handle ownership.

The requested mode is preferred, followed by common sizes at its rate and 60 Hz,
deduplicated. All mode callbacks agree. EDID DTDs are omitted above 655.35 MHz or
12-bit dimensions; range fields saturate at EDID limits. The IddCx callbacks supply
the complete list, with exact refresh rates and 64-bit pixel rates.

## Build

Install MSVC x64 Build Tools and SDK/WDK **10.0.26100.0**, then run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build.ps1 `
    -OutDir D:\sunna-dev\driver-build -Version 0.1.0.0
```

The script discovers MSVC with `vswhere`, imports `vcvars64.bat`, and invokes
`cl.exe`/`link.exe` directly. It runs mode/EDID/ABI checks, stamps the INF with today's
local date, runs `InfVerif /u` when available, and creates the catalog with
`Inf2Cat /os:10_VB_X64`. All outputs stay in OutDir. Use a dedicated output directory.
A usable `CN=Sunna Test Signing` code-signing certificate in CurrentUser\My or
LocalMachine\My signs the DLL **before catalog generation**, then the catalog.
The build never creates or trusts a certificate. Without one it produces an
unsigned package and prints the remaining step.

Compatibility/build decisions:

- **Windows 10 2004 (19041)+ and Windows 11 x64**: UMDF 2.31 (minimum 31),
  IddCx 1.4 (`IDDCX_VERSION_MINOR=4`, `IDDCX_MINIMUM_VERSION_REQUIRED=4`). See
  [UMDF versions](https://learn.microsoft.com/en-us/windows-hardware/drivers/wdf/umdf-version-history)
  and [IddCx 1.4 compatibility](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/building-iddcx1.4-drivers).
  `UmdfExtensions=IddCx0102` is the IddCx extension service identifier, not a request
  to link IddCx 1.2; the 1.4 stub and version defines select the binding version.
- **Hybrid CRT** matches `Universal.UserMode.props` and the WDK
  `WindowsDriver.Common.targets` `SetHybridCrtCl/SetHybridCrtLink` rules:
  their `/MD` default becomes `/MT`, with `/NODEFAULTLIB:libucrt.lib ucrt.lib`.
  MSVC runtime support is static; UCRT is supplied by Windows. No VC redistributable
  DLL dependency. OneCore libraries are used. WDK/SDK headers are external headers
  with their warnings suppressed; Sunna source uses `/W4 /WX`.
- **Entry/export**: `WdfDriverStubUm.lib` supplies exported `FxDriverEntryUm`, which
  calls the driver's C-linkage `DriverEntry`. The normal DLL CRT entry point stays
  intact; no `/ENTRY:DriverEntry`. `IddCxStub.lib` supplies class-extension binding.
  The build explicitly roots `/EXPORT:FxDriverEntryUm` and enables `/GS`, CFG, ASLR,
  high-entropy ASLR and NX. `/Qspectre` is enabled only if the OneCore Spectre CRT
  libraries exist.
## Test installation (administrator; manual only)

These steps change Windows configuration; `build.ps1` does not perform them.
Enable test signing and reboot. Secure Boot may need to be disabled in firmware
if it prevents changing this boot option.

```powershell
bcdedit /set testsigning on
# Reboot, then create a certificate only if one does not already exist:
$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=Sunna Test Signing' `
    -CertStoreLocation Cert:\CurrentUser\My -HashAlgorithm SHA256
Export-Certificate -Cert $cert -FilePath D:\sunna-dev\driver-build\SunnaTest.cer
Import-Certificate -FilePath D:\sunna-dev\driver-build\SunnaTest.cer -CertStoreLocation Cert:\LocalMachine\Root
Import-Certificate -FilePath D:\sunna-dev\driver-build\SunnaTest.cer -CertStoreLocation Cert:\LocalMachine\TrustedPublisher
# Rerun build.ps1 to sign DLL + regenerated catalog with that certificate.
pnputil /add-driver D:\sunna-dev\driver-build\SunnaDisplay.inf /install
```

Let `sunnad` create its SwDevice, or create a persistent manual test device:

```powershell
& 'C:\Program Files (x86)\Windows Kits\10\Tools\10.0.26100.0\x64\devcon.exe' `
    install D:\sunna-dev\driver-build\SunnaDisplay.inf 'Root\SunnaDisplay'
```

Send ADD to create a display. Test 1080p60/144/240, 4K240, four monitors/rejected
fifth, malformed requests, REMOVE/re-ADD, daemon exit, sleep/resume and GPU loss.
Measure Desktop Duplication with animation and check cursor-free pixels. The host
should confirm/set the active mode when Windows restores saved settings. Diagnostics
use `OutputDebugStringW` with the `SunnaDisplay:` prefix.

## Remove

Stop `sunnad` (closing the SwDevice handle). For a devcon-created device:

```powershell
& 'C:\Program Files (x86)\Windows Kits\10\Tools\10.0.26100.0\x64\devcon.exe' remove 'Root\SunnaDisplay'
pnputil /enum-drivers  # Find SunnaDisplay.inf's published oemNN.inf name.
pnputil /delete-driver oemNN.inf /uninstall
# Replace THUMBPRINT with the exact test certificate thumbprint used above:
Remove-Item Cert:\LocalMachine\Root\THUMBPRINT
Remove-Item Cert:\LocalMachine\TrustedPublisher\THUMBPRINT
Remove-Item Cert:\CurrentUser\My\THUMBPRINT
# If its private key was instead in LocalMachine\My, remove it there.
bcdedit /set testsigning off
# Reboot. Remove SunnaTest.cer and the build output directory if no longer needed.
```

Do not remove a certificate still used by another test driver. Restore any firmware
setting changed for testing. Build/package checks do not verify driver load, device
lifecycle, cursor separation, or sustained capture rates; these need the install test.
