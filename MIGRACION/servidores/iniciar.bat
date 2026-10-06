@echo off
rem Arranca el servidor de Luna y deja la ventana abierta cuando se para.
rem Se le pueden pasar opciones:  iniciar.bat --puerto 47600 --nombre "La Base"
title Servidor de Luna
cd /d "%~dp0"
LunaServidor.exe %*
echo.
echo El servidor se ha parado. Pulsa una tecla para cerrar esta ventana.
pause >nul
