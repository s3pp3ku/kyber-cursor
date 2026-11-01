$ErrorActionPreference = 'Stop'

function Invoke-Cargo {
    param([string[]]$Arguments)
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) { throw "cargo failed: $($Arguments -join ' ')" }
}

$rustRoot = Split-Path $PSScriptRoot -Parent
$repoRoot = Split-Path $rustRoot -Parent
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
$output = Join-Path $repoRoot "test-artifacts/windows-desktop/$stamp"
$bundle = Join-Path $output 'kyber-windows-desktop-x64'
$previous = Get-Location
$previousFlags = $env:RUSTFLAGS

try {
    Set-Location $rustRoot
    # The receiving PC needs only Windows system DLLs, not a separate VC runtime.
    $env:RUSTFLAGS = "$previousFlags -C target-feature=+crt-static".Trim()
    Invoke-Cargo -Arguments @('build', '--locked', '--release', '--target', 'x86_64-pc-windows-msvc', '-p', 'kyber-windows')
    New-Item -ItemType Directory -Path $bundle -Force | Out-Null
    $frames = Join-Path $bundle 'frames'
    Invoke-Cargo -Arguments @('run', '--locked', '--release', '-p', 'kyber-core', '--example', 'windows_frames', '--', $frames)
    $exe = Join-Path $bundle 'kyber-cursor.exe'
    Copy-Item (Join-Path $rustRoot 'target/x86_64-pc-windows-msvc/release/kyber-cursor.exe') $exe
    Copy-Item (Join-Path $rustRoot 'crates/kyber-windows/READ-ME.txt') (Join-Path $bundle 'READ-ME.txt')
    $command = Get-Content -Raw (Join-Path $rustRoot 'crates/kyber-windows/Restore Windows Cursor.cmd')
    [IO.File]::WriteAllText((Join-Path $bundle 'Restore Windows Cursor.cmd'), ($command -replace "`r?`n", "`r`n"), [Text.Encoding]::ASCII)
    Copy-Item (Join-Path $repoRoot 'assets/sounds/manifest.json') (Join-Path $bundle 'sound-pack.json')
    Copy-Item (Join-Path $repoRoot 'assets/sounds/README.md') (Join-Path $bundle 'AUDIO-NOTICE.md')
    Copy-Item (Join-Path $repoRoot 'LICENSE') (Join-Path $bundle 'CODE-LICENSE.txt')
    Copy-Item (Join-Path $repoRoot 'THIRD_PARTY_NOTICES.md') (Join-Path $bundle 'THIRD-PARTY-NOTICES.md')
    @{
        platform = 'Windows 10/11 x64'
        target = 'x86_64-pc-windows-msvc'
        scope = 'Desktop-wide standard system cursor replacement; no demo window'
        sound_pack = 'kyber-private-clips-v1'
        executable_sha256 = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLowerInvariant()
        sound_manifest_sha256 = (Get-FileHash (Join-Path $repoRoot 'assets/sounds/manifest.json') -Algorithm SHA256).Hash.ToLowerInvariant()
        tests_run = $false
        native_windows_interactive_validation_completed = $false
        native_windows_audio_approval_completed = $false
        native_windows_recovery_validation_completed = $false
    } | ConvertTo-Json | Set-Content (Join-Path $bundle 'build-info.json') -Encoding utf8
    $archive = Join-Path $output 'kyber-windows-desktop-x64.zip'
    Compress-Archive -Path $bundle -DestinationPath $archive
    $hash = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText("$archive.sha256", "$hash  kyber-windows-desktop-x64.zip`n")
    Write-Output "Windows desktop package: $archive"
} finally {
    $env:RUSTFLAGS = $previousFlags
    Set-Location $previous
}
