[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$ProgressPreference = 'SilentlyContinue'
$release = Invoke-RestMethod -Method Get -Uri "https://api.github.com/repos/ducaale/xh/releases/latest"
$asset = $release.assets | Where-Object name -Like *.zip
$destDir = "$home\bin\"
$zipFile = "$env:TEMP\$($asset.name)"

Write-Output "Downloading: $($asset.name)"
Invoke-RestMethod -Method Get -Uri $asset.browser_download_url -OutFile $zipFile

# checks if an older version of xh.exe exists in '$destDir', if yes, then delete it, if not, then download latest zip to extract from
$xhPath = "${destDir}xh.exe"
if (Test-Path $xhPath -PathType Leaf) {
	"`n xh.exe exists in $destDir, deleting..."
	rm -r -fo $xhPath
} else {
	"`n ${destDir}xh.exe does not exist"
}

#xh.exe extraction start
Add-Type -Assembly System.IO.Compression.FileSystem

$zip = [IO.Compression.ZipFile]::OpenRead($zipFile)
$entries=$zip.Entries | Where {$_.FullName -Like '*.exe'}

#create dir for result of extraction
New-Item -ItemType Directory -Path $destDir -Force

#extraction
$entries | foreach {[IO.Compression.ZipFileExtensions]::ExtractToFile( $_, $destDir + $_.Name) }

#free zipfile
$zip.Dispose()

Remove-Item -Path $zipFile

Write-Host "`n Extracted 'xh.exe' file to $destDir"

# Add to environment variables
$p = [System.Environment]::GetEnvironmentVariable('Path', [System.EnvironmentVariableTarget]::User);
if (!$p.ToLower().Contains($destDir.ToLower()))
{
	Write-Output "`n Adding $destDir to your Path"

	$p += ";$destDir";
	[System.Environment]::SetEnvironmentVariable('Path',$p,[System.EnvironmentVariableTarget]::User);

	$Env:Path = [System.Environment]::GetEnvironmentVariable('Path', [System.EnvironmentVariableTarget]::Machine) + ";" + $p

	Write-Host "`n Modified PATH environment variable. Restart programs which use command line." -Foreground yellow
}

# visual redistributor version check for vcruntime140.dll for xh
$currentVSR = Get-WmiObject -Class Win32_Product -Filter "Name LIKE '%Microsoft Visual C++ 20%'"

# Finding if your VS C++ Runtime Redistributable year is 2015 or higher
$currentYear = Get-Date -Format yy
$currentVSRMinimum
$lowestVSRyear
$highestVSyear

for ($i = 15; $i -le $currentYear;$i++){
	$VSRyear = $currentVSR.name -Match "20$i"
	$VSRyearmin = $VSRyear -Match "Minimum"

	if ($VSRyearmin) {
		$highestVSyear = '20$i'

		$currentVSRMinimum = $currentVSR.name -Match "20$i"
	}

	if ($VSRyearmin -eq $currentVSR.name -Match "2015") {
		"`n Found VSR with year version 2015"
		$lowestVSRyear = 2015
	}
}

if ($lowestVSRyear) {
	Write-Host "`n You have 2015 VSR: $currentVSRMinimum"
} elseif ($highestVSyear) {
	Write-Host "`n Highest version: $currentVSRMinimum"
} else {
	Write-Host "`n Could not find Visual Studio Redistributable 2015 or higher"
	Write-Host "`n Installing Visual Studio 2015-2022 Redistributable..."

	# VS C++ Runtime Redistributable 2015-2022
	$url = "https://aka.ms/vs/17/release/vc_redist.x64.exe"
	$outpath = "$env:TEMP\vc_redist.x64.exe"

	# progress preference is for disabling progress bar, makes download faster for some reason
	$ProgressPreference = 'SilentlyContinue'
	Invoke-WebRequest -Uri $url -OutFile $outpath

	$args = "/S","/v","/qn"
	# Requires admin
	$VSRinstallerprocess = Start-Process $outpath -ArgumentList $args -PassThru
	$VSRinstallerprocess

	# Delete installer from downloaded path after installer process is done
	$VSRinstallerprocess.WaitForExit()
	Remove-Item -Path $outpath -Force -Recurse
}

Write-Output "`n Done!"
Write-Host -NoNewLine "`n Press any key to continue...";
$null = $Host.UI.RawUI.ReadKey('NoEcho,IncludeKeyDown');