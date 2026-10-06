@echo off
rem Arranca el servidor de Selene y deja la ventana abierta cuando se para.
rem Se le pueden pasar opciones:  iniciar.bat --puerto 47600 --nombre "La Base"
title Servidor de Selene
cd /d "%~dp0"
SeleneServidor.exe %*
echo.
echo El servidor se ha parado. Pulsa una tecla para cerrar esta ventana.
pause >nul
