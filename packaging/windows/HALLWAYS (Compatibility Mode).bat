@echo off
rem If the game still crashes on start: low graphics plus the Vulkan renderer.
cd /d "%~dp0"
set WGPU_BACKEND=vulkan
start "" hallways.exe --low
