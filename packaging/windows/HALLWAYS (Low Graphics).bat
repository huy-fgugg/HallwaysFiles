@echo off
rem Starts HALLWAYS with low graphics for older or integrated GPUs.
cd /d "%~dp0"
start "" hallways.exe --low
