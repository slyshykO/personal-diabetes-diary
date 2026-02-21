@echo off

REM %~dp0

setlocal
set saved_dir=%cd%

cd /d %~dp0


set wsl_dir=%~dp0
set wsl_dir=%wsl_dir:\=/%
set wsl_dir=%wsl_dir:C:=/mnt/c%
set wsl_dir=%wsl_dir:D:=/mnt/d%

wsl -d WLinux -u alex --shell-type login "%wsl_dir%build-wsl.sh"  || goto :error

cd /d %saved_dir%
exit /b 0

:error
cd /d %saved_dir%
exit /b 1