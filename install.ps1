[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$ProgressPreference = 'SilentlyContinue'
$release = Invoke-RestMethod -Method Get -Uri "https://api.github.com/repos/ducaale/xh/releases/latest"
$asset = $release.assets | Where-Object name -like *.zip
$destdir = "$home\bin\"
$zipfile = "$env:TEMP\$($asset.name)"

Write-Output "Downloading: $($asset.name)"
Invoke-RestMethod -Method Get -Uri $asset.browser_download_url -OutFile $zipfile

# checks if an older version of xh.exe exists in '$destdir', if yes, then delete it, if not, then download latest zip to extract from
$xhPath = "${destdir}xh.exe"
if (Test-Path -Path $xhPath -PathType Leaf) {
    "`n xh.exe exists in $destdir, deleting..."
	rm -r -fo $xhPath
}

#xh.exe extraction start
Add-Type -Assembly System.IO.Compression.FileSystem

$zip = [IO.Compression.ZipFile]::OpenRead($zipfile)
$entries=$zip.Entries | where {$_.FullName -like '*.exe'} 

#create dir for result of extraction
New-Item -ItemType Directory -Path $destdir -Force | Out-Null

#extraction
$entries | foreach {[IO.Compression.ZipFileExtensions]::ExtractToFile( $_, $destdir + $_.Name) }
#free zipfile
$zip.Dispose()

Remove-Item -Path $zipfile

Write-Host "`n Exctracted 'xh.exe' file is located in: $destdir."

# Requires powershell to be run as administrator. Creates 'xhs' symbolic link
cmd /c mklink /d xhs $xhPath

# Add to environment variables
$p = [System.Environment]::GetEnvironmentVariable('Path', [System.EnvironmentVariableTarget]::User);
if (!$p.ToLower().Contains($destdir.ToLower()))
{
	Write-Output "`n Adding $destdir to your Path"
	
	$p += "$destdir";
	[System.Environment]::SetEnvironmentVariable('Path',$p,[System.EnvironmentVariableTarget]::User);
	
	$Env:Path = [System.Environment]::GetEnvironmentVariable('Path', [System.EnvironmentVariableTarget]::Machine) + ";" + $p
	
	Write-Host "`n PATH environment variable changed (restart your applications that use command line)." -foreground yellow
}

Write-Output "`n Done!"
Write-Host -NoNewLine "`n Press any key to continue...";
$null = $Host.UI.RawUI.ReadKey('NoEcho,IncludeKeyDown');