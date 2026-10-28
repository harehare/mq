# mq installation script for Windows
#
#   powershell -c "irm https://mqlang.org/install.ps1 | iex"
#
# Options are passed as environment variables because `irm | iex` cannot take arguments:
#   MQ_VERSION     Release tag to install (e.g. v0.9.3). Defaults to the latest release.
#   MQ_HOME        Installation directory. Defaults to $HOME\.local\bin.
#   MQ_WITH_DEBUG  Set to 1 to also install mq-dbg.
#   MQ_NO_MODIFY_PATH  Set to 1 to leave the user PATH untouched.
#
# Keep this file ASCII only: Windows PowerShell 5.1 decodes `irm` output without a charset as Latin-1.

function Install-Mq {
    $ErrorActionPreference = 'Stop'
    # Invoke-WebRequest is very slow on Windows PowerShell 5.1 while the progress bar is drawn.
    $ProgressPreference = 'SilentlyContinue'

    $repo = 'harehare/mq'
    $Version = $env:MQ_VERSION
    $WithDebug = $env:MQ_WITH_DEBUG -eq '1'
    $NoModifyPath = $env:MQ_NO_MODIFY_PATH -eq '1'
    $InstallDir = if ($env:MQ_HOME) { $env:MQ_HOME } else { Join-Path $HOME '.local\bin' }

    function Write-Log([string]$Message) { Write-Host "[mq] $Message" -ForegroundColor Green }
    function Write-Warn([string]$Message) { Write-Host "[mq] warning: $Message" -ForegroundColor Yellow }

    # Windows PowerShell 5.1 does not enable TLS 1.2 by default.
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    } catch {
        Write-Warn "Could not enable TLS 1.2: $($_.Exception.Message)"
    }

    function Get-Target {
        $arch = $null
        try {
            $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
        } catch {
            # Older .NET Framework: PROCESSOR_ARCHITEW6432 is set when a 32-bit process runs on 64-bit Windows.
            $arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
        }
        switch -Regex ($arch) {
            '^(X64|AMD64)$' { return 'x86_64-pc-windows-msvc' }
            '^(Arm64|ARM64)$' { return 'aarch64-pc-windows-msvc' }
            default { throw "Unsupported architecture: $arch" }
        }
    }

    function Get-ReleaseUrl([string]$FileName) {
        if ($Version) {
            return "https://github.com/$repo/releases/download/$Version/$FileName"
        }
        return "https://github.com/$repo/releases/latest/download/$FileName"
    }

    # checksums.txt lines look like "<sha256>  <artifact dir>/<file name>".
    function Get-ExpectedChecksum([string]$Checksums, [string]$FileName) {
        foreach ($line in ($Checksums -split "`n")) {
            $parts = $line.Trim() -split '\s+', 2
            if ($parts.Count -ne 2) { continue }
            $name = $parts[1].TrimStart('*')
            if ($name -eq $FileName -or $name.EndsWith("/$FileName")) { return $parts[0] }
        }
        return $null
    }

    function Install-Binary([string]$Name, [string]$Target, [string]$Checksums, [string]$TempDir) {
        $fileName = "$Name-$Target.exe"
        $url = Get-ReleaseUrl $fileName
        $tempFile = Join-Path $TempDir $fileName

        Write-Log "Downloading $url"
        Invoke-WebRequest -Uri $url -OutFile $tempFile -UseBasicParsing

        $expected = Get-ExpectedChecksum $Checksums $fileName
        if (-not $expected) { throw "No checksum found for $fileName" }
        $actual = (Get-FileHash -Path $tempFile -Algorithm SHA256).Hash
        if ($actual -ne $expected) {
            throw "Checksum verification failed for $fileName`nExpected: $expected`nGot:      $actual"
        }
        Write-Log "Checksum verified for $fileName"

        $dest = Join-Path $InstallDir "$Name.exe"
        try {
            Move-Item -Path $tempFile -Destination $dest -Force
        } catch {
            throw "Failed to write $dest. If $Name is running, close it and try again. $($_.Exception.Message)"
        }
        Write-Log "$Name installed to $dest"
    }

    function Add-ToUserPath([string]$Dir) {
        # Read and write the registry value directly so an existing REG_EXPAND_SZ Path keeps its %VARS%.
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
        try {
            $current = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            $entries = @($current -split ';' | Where-Object { $_ })
            if ($entries | Where-Object { $_.TrimEnd('\') -ieq $Dir.TrimEnd('\') }) {
                Write-Log "$Dir is already in your PATH"
            } else {
                $key.SetValue('Path', (($entries + $Dir) -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
                Write-Log "Added $Dir to your user PATH"
            }
        } finally {
            $key.Close()
        }
        if (($env:Path -split ';') -notcontains $Dir) { $env:Path = "$env:Path;$Dir" }
    }

    $target = Get-Target
    Write-Log "Detected system: windows/$target"

    $tempDir = Join-Path ([IO.Path]::GetTempPath()) ("mq-install-" + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
    try {
        Write-Log 'Downloading checksums'
        $checksums = (Invoke-WebRequest -Uri (Get-ReleaseUrl 'checksums.txt') -UseBasicParsing).Content
        if ($checksums -is [byte[]]) { $checksums = [Text.Encoding]::UTF8.GetString($checksums) }

        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        Install-Binary 'mq' $target $checksums $tempDir
        if ($WithDebug) { Install-Binary 'mq-dbg' $target $checksums $tempDir }
    } finally {
        Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }

    if (-not $NoModifyPath) { Add-ToUserPath $InstallDir }

    $installed = & (Join-Path $InstallDir 'mq.exe') --version
    if ($LASTEXITCODE -ne 0) { throw 'mq was installed but failed to run' }

    Write-Host ''
    Write-Log "$installed installed successfully"
    Write-Host '  Restart your terminal, then run:'
    Write-Host '    mq --version'
    Write-Host "    echo '# Hello World' | mq '.h'"
    Write-Host '  Documentation: https://mqlang.org/'
}

# `exit` would close the user's shell under `irm | iex`, so failures are reported with throw.
Install-Mq
