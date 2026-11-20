# Fail closed on untrusted owners, writable access rules or reparse points.
$ErrorActionPreference = 'Stop'
$sidType = [System.Security.Principal.SecurityIdentifier]
$trusted = @(
  [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value,
  'S-1-5-18', 'S-1-5-32-544',
  'S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464'
)
function Check-Item($item) {
  if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'reparse point' }
  $acl = Get-Acl -LiteralPath $item.FullName
  if ($acl.GetOwner($sidType).Value -notin $trusted) { throw 'untrusted owner' }
  $raw = New-Object System.Security.AccessControl.RawSecurityDescriptor($acl.Sddl)
  if ($null -eq $raw.DiscretionaryAcl) { throw 'unrestricted ACL' }
  foreach ($rule in $acl.GetAccessRules($true, $true, $sidType)) {
    if ($rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly) { continue }
    # Write data/attributes/EA, append, delete child, delete, write ACL/owner.
    if ($rule.AccessControlType -eq 'Allow' -and ($rule.FileSystemRights.value__ -band 0x500D0156) -and $rule.IdentityReference.Value -notin $trusted) {
      throw 'writable by another principal'
    }
  }
}
try {
  $path = [IO.Path]::GetFullPath($env:SUNNA_ACL_CHECK)
  while ($path) {
    if (Test-Path -LiteralPath $path) { Check-Item (Get-Item -Force -LiteralPath $path) }
    $parent = [IO.Directory]::GetParent($path)
    $path = if ($parent) { $parent.FullName } else { $null }
  }
  if ($env:SUNNA_ACL_TREE -eq '1') {
    Get-ChildItem -LiteralPath $env:SUNNA_ACL_CHECK -Force -Recurse | ForEach-Object { Check-Item $_ }
  }
} catch { exit 1 }
