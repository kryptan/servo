
cd "C:\Program Files (x86)\Microsoft Visual Studio\2017\BuildTools\"
@call VC\Auxiliary\Build\vcvarsall.bat x64
cd C:\projects\piautos\servo
mach build -r