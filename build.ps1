param([ValidateSet('Check','Test','Build','All')][string]$Task = 'All')
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
$oldFlags = $env:CARGO_ENCODED_RUSTFLAGS
$oldLinker = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER
try {
    # Prefer the optional local SDK if prepared; otherwise use VS Developer PowerShell.
    $sdkRoot = Join-Path $PSScriptRoot 'target\tooling\winsysroot'
    if (Test-Path -LiteralPath $sdkRoot) {
        $sysroot = (& rustc --print sysroot).Trim()
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = Join-Path $sysroot 'lib\rustlib\x86_64-pc-windows-msvc\bin\rust-lld.exe'
        $flags = @('-Clinker-flavor=lld-link','-Ctarget-feature=+crt-static')
        $crt = Get-ChildItem (Join-Path $sdkRoot 'VC\Tools\MSVC') -Directory | Sort-Object Name -Descending | Select-Object -First 1
        $sdk = Get-ChildItem (Join-Path $sdkRoot 'Windows Kits\10\Lib') -Directory | Sort-Object Name -Descending | Select-Object -First 1
        foreach ($lib in @((Join-Path $crt.FullName 'lib\x64'), (Join-Path $sdk.FullName 'um\x64'), (Join-Path $sdk.FullName 'ucrt\x64'))) {
            $flags += '-Lnative=' + $lib
        }
        $env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]31
    }
    function Invoke-Cargo([string[]]$Arguments) {
        & cargo @Arguments
        if ($LASTEXITCODE -ne 0) { throw "cargo $Arguments failed ($LASTEXITCODE)" }
    }
    if ($Task -in @('Check','All')) {
        Invoke-Cargo @('fmt','--','--check')
        Invoke-Cargo @('clippy','--locked','--offline','--all-targets','--','-D','warnings')
    }
    if ($Task -in @('Test','All')) { Invoke-Cargo @('test','--locked','--offline') }
    if ($Task -in @('Build','All')) { Invoke-Cargo @('build','--release','--locked','--offline') }
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $oldFlags
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = $oldLinker
    Pop-Location
}
