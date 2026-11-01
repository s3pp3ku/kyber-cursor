param([switch]$SelfCheck)
$ErrorActionPreference = 'Stop'

function Invoke-Cargo {
    param([string[]]$Arguments)
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) { throw "cargo failed: $($Arguments -join ' ')" }
}

$rustRoot = Split-Path $PSScriptRoot -Parent
$repoRoot = Split-Path $rustRoot -Parent
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
$output = Join-Path $repoRoot "test-artifacts/windows-native-ci/$stamp"
$bundle = Join-Path $output 'kyber-windows-preview'
$previous = Get-Location

try {
    Set-Location $rustRoot
    Invoke-Cargo -Arguments @('fmt', '--all', '--check')
    Invoke-Cargo -Arguments @('test', '--locked', '-p', 'kyber-core', '-p', 'kyber-cli', '-p', 'kyber-preview')
    Invoke-Cargo -Arguments @('clippy', '--locked', '-p', 'kyber-core', '-p', 'kyber-cli', '-p', 'kyber-preview', '--all-targets', '--', '-D', 'warnings')
    Invoke-Cargo -Arguments @('build', '--locked', '--release', '--target', 'x86_64-pc-windows-msvc', '-p', 'kyber-preview')

    New-Item -ItemType Directory -Path $bundle -Force | Out-Null
    $frames = Join-Path $bundle 'frames'
    Invoke-Cargo -Arguments @('run', '--locked', '--release', '-p', 'kyber-core', '--example', 'glow_review', '--', $frames)
    $exe = Join-Path $bundle 'kyber-preview.exe'
    Copy-Item (Join-Path $rustRoot 'target/x86_64-pc-windows-msvc/release/kyber-preview.exe') $exe
    Copy-Item (Join-Path $rustRoot 'crates/kyber-preview/TESTING.txt') (Join-Path $bundle 'READ-ME.txt')
    Copy-Item (Join-Path $repoRoot 'assets/sounds/manifest.json') (Join-Path $bundle 'sound-pack.json')
    Copy-Item (Join-Path $repoRoot 'assets/sounds/README.md') (Join-Path $bundle 'AUDIO-NOTICE.md')
    Copy-Item (Join-Path $repoRoot 'LICENSE') (Join-Path $bundle 'CODE-LICENSE.txt')
    Copy-Item (Join-Path $repoRoot 'THIRD_PARTY_NOTICES.md') (Join-Path $bundle 'THIRD-PARTY-NOTICES.md')

    if ($SelfCheck) {
        $report = Join-Path $bundle 'self-check.json'
        $process = Start-Process -FilePath $exe -ArgumentList @('--self-check', '--report', "`"$report`"") -Wait -PassThru
        if ($process.ExitCode -ne 0 -or !(Test-Path $report)) { throw 'Native cursor creation check failed' }
        Get-Content $report
    }

    @{
        platform = 'Windows x64'
        target = 'x86_64-pc-windows-msvc'
        native_cursor_creation_checked = [bool]$SelfCheck
        interactive_test_completed = $false
        sound_pack = 'kyber-private-clips-v1'
        audio = 'Shared embedded recordings, including held hum; no generated fallback'
        sound_pack_manifest_sha256 = (Get-FileHash (Join-Path $repoRoot 'assets/sounds/manifest.json') -Algorithm SHA256).Hash.ToLowerInvariant()
        executable_sha256 = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLowerInvariant()
        scope = 'Window-scoped cursor trial; desktop-wide backend and macOS remain unfinished'
    } | ConvertTo-Json | Set-Content (Join-Path $bundle 'build-info.json') -Encoding utf8

    $archive = Join-Path $output 'kyber-windows-preview.zip'
    Compress-Archive -Path $bundle -DestinationPath $archive
    Write-Output "Test package: $archive"
} finally {
    Set-Location $previous
}
