[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$ProgressPreference = 'SilentlyContinue'
$release = Invoke-RestMethod -Method Get -Uri "https://api.github.com/repos/ducaale/xh/releases/latest"
$asset = $release.assets | Where-Object name -like *.zip
$destdir = "$home\bin\"
$zipfile = "$env:TEMP\$($asset.name)"

Write-Output "Downloading: $($asset.name)"
Invoke-RestMethod -Method Get -Uri $asset.browser_download_url -OutFile $zipfile

# checks if an older version of xh.exe (includes xhs.exe) exists in '$destdir', if yes, then delete it, if not, then download latest zip to extract from
$xhPath = "${destdir}xh.exe"
$xhsPath = "${destdir}xhs.exe"
if (Test-Path -Path $xhPath -PathType Leaf) {
    "`n xh.exe exists in $destdir, deleting xh and xhs"
	rm -r -fo $xhPath
	rm -r -fo $xhsPath
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

# Copy xh.exe as xhs.exe into bin
Copy-Item $xhPath $xhsPath

Write-Host "`n Exctracted 'xh.exe' file is located in: $destdir."

# Add to environment variables
$p = [System.Environment]::GetEnvironmentVariable('Path', [System.EnvironmentVariableTarget]::User);
if (!$p.ToLower().Contains($destdir.ToLower()))
{
	# path to "user"/bin
	Write-Output "`n Adding $destdir to your Path"
	
	$p += "$destdir";
	[System.Environment]::SetEnvironmentVariable('Path',$p,[System.EnvironmentVariableTarget]::User);
	
	$Env:Path = [System.Environment]::GetEnvironmentVariable('Path', [System.EnvironmentVariableTarget]::Machine) + ";" + $p
	
	# Path to xhs.exe
	
	Write-Host "`n PATH environment variable changed (restart your applications that use command line)." -foreground yellow
}

Write-Output "`n Done!"
