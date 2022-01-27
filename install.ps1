[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$ProgressPreference = 'SilentlyContinue'
$release = Invoke-RestMethod -Method Get -Uri "https://api.github.com/repos/ducaale/xh/releases/latest"
$asset = $release.assets | Where-Object name -like *x86_64-pc-windows*.zip
$destdir = "$home\bin\"
$zipfile = "$env:TEMP\$($asset.name)"
$zipfilename = [System.IO.Path]::GetFileNameWithoutExtension("$zipfile")

Write-Output "Downloading: $($asset.name)"
Invoke-RestMethod -Method Get -Uri $asset.browser_download_url -OutFile $zipfile

# Checks if an older version of xh.exe (includes xhs.exe) exists in '$destdir', if yes, then delete it, if not, then download latest zip to extract from.

$xhPath = "${destdir}xh.exe"
$xhsPath = "${destdir}xhs.exe"
if (Test-Path -Path $xhPath -PathType Leaf) {
    "Removing previous installation of xh from $($destdir)"
    Remove-Item -r -fo $xhPath
    Remove-Item -r -fo $xhsPath
}

# xh.exe extraction start.

Add-Type -Assembly System.IO.Compression.FileSystem

$zip = [IO.Compression.ZipFile]::OpenRead($zipfile)
$entries = $zip.Entries | Where-Object { $_.FullName -like '*.exe' }

# Create dir for result of extraction.

New-Item -ItemType Directory -Path $destdir -Force | Out-Null

# Extraction.

$entries | ForEach-Object { [IO.Compression.ZipFileExtensions]::ExtractToFile( $_, $destdir + $_.Name) }

# Free the zipfile.

$zip.Dispose()

Remove-Item -Path $zipfile

# Copy xh.exe as xhs.exe into bin.

Copy-Item $xhPath $xhsPath

# Add to environment variables.

$p = [System.Environment]::GetEnvironmentVariable('Path', [System.EnvironmentVariableTarget]::User)
if (!$p.ToLower().Contains($destdir.ToLower())) {

    # Path to "user"/bin.

    Write-Output "Adding $destdir to your Path"
	
    $p += "$destdir"
    [System.Environment]::SetEnvironmentVariable('Path', $p, [System.EnvironmentVariableTarget]::User)
    $Env:Path = [System.Environment]::GetEnvironmentVariable('Path', [System.EnvironmentVariableTarget]::Machine) + ";" + $p
	
    # Path to xhs.exe.

    Write-Host "PATH environment variable changed (restart your applications that use command line)." -foreground yellow
}

# Get version from zip file name.

$xhVersion = $($zipfilename.trim("xh-v -x86_64-pc-windows-msvc.zip"))
Write-Output "xh v$($xhVersion) has been installed to:`n - $xhPath`n - $xhsPath"