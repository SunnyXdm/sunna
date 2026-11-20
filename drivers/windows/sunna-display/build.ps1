[CmdletBinding()]
param(
    [string]$OutDir = 'D:\sunna-dev\driver-build',
    [ValidatePattern('^\d+\.\d+\.\d+\.\d+$')]
    [string]$Version = '0.1.0.0'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Invoke-Checked([string]$Exe, [string[]]$Arguments) {
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Exe failed with exit code $LASTEXITCODE" }
}
function Require-File([string]$Path) {
    if (!(Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Required build file missing: $Path" }
    return $Path
}

foreach ($part in $Version.Split('.')) {
    if ([uint64]$part -gt 65535) { throw 'Version components must be between 0 and 65535.' }
}
$source = $PSScriptRoot
$OutDir = [IO.Path]::GetFullPath($OutDir).TrimEnd('\')
$repo = Split-Path (Split-Path (Split-Path $source -Parent) -Parent) -Parent
if ($OutDir -eq $repo -or $OutDir.StartsWith($repo + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'OutDir must be outside the synced repository.'
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$kit = "${env:ProgramFiles(x86)}\Windows Kits\10"
$sdk = '10.0.26100.0'
$umdf = '2.31'
$iddcx = '1.4'
$vswhere = Require-File "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -property installationPath
if ($LASTEXITCODE -ne 0 -or !$vs) { throw 'Visual Studio Build Tools with the x64 C++ tools were not found.' }
$vcvars = Require-File "$vs\VC\Auxiliary\Build\vcvars64.bat"
# Capture the developer environment without needing WDK Visual Studio integration.
$environment = & $env:ComSpec /d /c "call `"$vcvars`" $sdk >nul && set"
if ($LASTEXITCODE -ne 0) { throw 'vcvars64.bat failed. Install the MSVC x64 tools and SDK 10.0.26100.0.' }
foreach ($line in $environment) {
    if ($line -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
}
$compiler = Require-File "$env:VCToolsInstallDir\bin\Hostx64\x64\cl.exe"
$linker = Require-File "$env:VCToolsInstallDir\bin\Hostx64\x64\link.exe"
$stamp = Require-File "$kit\bin\$sdk\x64\stampinf.exe"
$inf2cat = Require-File "$kit\bin\$sdk\x86\Inf2Cat.exe"
$signTool = Require-File "$kit\bin\$sdk\x64\signtool.exe"
$iddInclude = "$kit\Include\$sdk\um\iddcx\$iddcx"
$wdfInclude = "$kit\Include\wdf\umdf\$umdf"
$iddLib = Require-File "$kit\Lib\$sdk\um\x64\iddcx\$iddcx\IddCxStub.lib"
$wdfLib = Require-File "$kit\Lib\wdf\umdf\x64\$umdf\WdfDriverStubUm.lib"
$null = Require-File "$iddInclude\IddCx.h"
$null = Require-File "$wdfInclude\wdf.h"

# Match WDK Universal.UserMode.props + SetHybridCrtCl/SetHybridCrtLink:
# /MD is converted to /MT, then the static UCRT is replaced by the OS UCRT.
$crtLib = "$env:VCToolsInstallDir\lib\onecore\x64"
$spectreLib = "$env:VCToolsInstallDir\lib\spectre\onecore\x64"
$spectre = (Test-Path "$spectreLib\libcmt.lib") -and (Test-Path "$spectreLib\libvcruntime.lib")
if ($spectre) { $crtLib = $spectreLib }
$null = Require-File "$crtLib\libcmt.lib"
$env:INCLUDE = "$iddInclude;$wdfInclude;$kit\Include\$sdk\um;$kit\Include\$sdk\shared;$kit\Include\$sdk\ucrt;$kit\Include\$sdk\winrt;$env:VCToolsInstallDir\include"
$env:LIB = "$crtLib;$kit\Lib\$sdk\um\x64;$kit\Lib\$sdk\ucrt\x64"
$compile = @('/nologo','/c','/std:c++17','/permissive-','/external:anglebrackets','/external:W0','/W4','/WX','/O2','/GS','/guard:cf',
    '/MT','/EHsc','/DUNICODE','/D_UNICODE','/DUMDF_USING_NTSTATUS','/DWIN32_LEAN_AND_MEAN',
    '/D_WIN32_WINNT=0x0A00','/DWINVER=0x0A00','/DUMDF_VERSION_MAJOR=2','/DUMDF_VERSION_MINOR=31',
    '/DUMDF_MINIMUM_VERSION_REQUIRED=31','/DIDDCX_VERSION_MAJOR=1','/DIDDCX_VERSION_MINOR=4',
    '/DIDDCX_MINIMUM_VERSION_REQUIRED=4',"/Fo$OutDir\SunnaDisplay.obj", "$source\driver.cpp")
if ($spectre) { $compile += '/Qspectre' }
Write-Host "Building x64 Sunna Display $Version (UMDF $umdf, IddCx $iddcx, hybrid CRT, Spectre=$spectre)"
# All tool side products (including InfVerif reports) stay outside the synced tree.
Push-Location $OutDir
try {
    Remove-Item "$OutDir\SunnaDisplay.cat" -Force -ErrorAction SilentlyContinue
    Invoke-Checked $compiler $compile
    Invoke-Checked $linker @('/nologo','/DLL','/MACHINE:X64','/SUBSYSTEM:WINDOWS,10.00',
        '/NXCOMPAT','/DYNAMICBASE','/HIGHENTROPYVA','/guard:cf','/INCREMENTAL:NO','/WX',
        '/EXPORT:FxDriverEntryUm','/NODEFAULTLIB:libucrt.lib','/NODEFAULTLIB:kernel32.lib',
        '/NODEFAULTLIB:user32.lib',"/OUT:$OutDir\SunnaDisplay.dll", "/IMPLIB:$OutDir\SunnaDisplay.lib",
        "$OutDir\SunnaDisplay.obj", $wdfLib, $iddLib,
        'ucrt.lib','OneCoreUAP.lib','ntdll.lib','d3d11.lib','dxgi.lib','avrt.lib')
    Invoke-Checked $compiler @('/nologo','/std:c++17','/W4','/WX','/O2','/MT','/EHsc',
        "/Fo$OutDir\modes_test.obj", "/Fe$OutDir\modes_test.exe", "$source\tests\modes_test.cpp",
        '/link','/NODEFAULTLIB:libucrt.lib','ucrt.lib','OneCoreUAP.lib')
    Invoke-Checked "$OutDir\modes_test.exe" @()
    Copy-Item "$source\SunnaDisplay.inf" "$OutDir\SunnaDisplay.inf" -Force
    Invoke-Checked $stamp @('-f',"$OutDir\SunnaDisplay.inf",'-d',((Get-Date).ToString('MM/dd/yyyy', [Globalization.CultureInfo]::InvariantCulture)),
        '-v',$Version,'-a','amd64','-u',"$umdf.0")
    $infverif = @("$kit\bin\$sdk\x86\infverif.exe", "$kit\bin\$sdk\x64\infverif.exe",
        "$kit\Tools\$sdk\x64\infverif.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
    if ($infverif) {
        Invoke-Checked $infverif @('/u','/v',"$OutDir\SunnaDisplay.inf")
        Write-Host 'InfVerif /u: PASS'
    } else { Write-Warning 'InfVerif not available; INF validation was skipped.' }

    $certificate = $null
    $machineStore = $false
    foreach ($store in @('Cert:\CurrentUser\My','Cert:\LocalMachine\My')) {
        $candidate = Get-ChildItem $store -CodeSigningCert | Where-Object {
            $_.Subject -eq 'CN=Sunna Test Signing' -and $_.HasPrivateKey -and
            $_.NotAfter -gt (Get-Date) -and $_.NotBefore -le (Get-Date)
        } | Sort-Object NotAfter -Descending | Select-Object -First 1
        if ($candidate) {
            $certificate = $candidate
            $machineStore = $store -like '*LocalMachine*'
            break
        }
    }
    $signArgs = @()
    if ($certificate) {
        $signArgs = @('sign','/fd','SHA256','/s','My','/sha1',$certificate.Thumbprint)
        if ($machineStore) { $signArgs += '/sm' }
        # Sign the DLL before catalog hashing; never invalidate a completed catalog.
        Invoke-Checked $signTool ($signArgs + @("$OutDir\SunnaDisplay.dll"))
    }
    Invoke-Checked $inf2cat @("/driver:$OutDir",'/os:10_VB_X64','/uselocaltime')
    if ($certificate) {
        Invoke-Checked $signTool ($signArgs + @("$OutDir\SunnaDisplay.cat"))
    } else {
        Write-Host 'Unsigned package: no usable CN=Sunna Test Signing code-signing certificate found.'
        Write-Host 'Create/trust a test certificate as described in README.md, then rerun this build to sign.'
    }
    Write-Host 'Build complete:'
    Get-Item "$OutDir\SunnaDisplay.dll", "$OutDir\SunnaDisplay.inf", "$OutDir\SunnaDisplay.cat" |
        Select-Object FullName, Length | Format-Table -AutoSize
} finally { Pop-Location }
