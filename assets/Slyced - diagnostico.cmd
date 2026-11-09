@echo off
rem Gera um relatorio do sistema e da placa de video (nao abre a janela do Slyced).
start "" "%~dp0Slyced.exe" --diagnostico
