#!/usr/bin/env bash
# Rebuild app/bin/runtime/gpu = whisper.cpp with Vulkan (graphics card), using WinLibs gcc + Vulkan tools from MSYS2.
# Dev helper — not for the user. Everything lives in %USERPROFILE%\PrismVoiceBuild (never under AppData).
# One-time inputs (already there on the dev PC, 2026-11-23):
#   dl/src/whisper.cpp            git clone --depth 1 --branch v1.9.5 https://github.com/ggml-org/whisper.cpp
#   dl/vk/root/ucrt64             repo.msys2.org/mingw/ucrt64 packages unpacked with Windows tar.exe:
#                                 vulkan-headers, vulkan-loader, shaderc, glslang, spirv-tools, spirv-headers
set -e
cd "$(dirname "$0")/.."
W=$(cygpath -u "$(ls -d "$LOCALAPPDATA/Microsoft/WinGet/Packages/"*WinLibs*/mingw64/bin | head -1)")
P="$USERPROFILE/PrismVoiceBuild"; V="$P/dl/vk/root/ucrt64"; VW=$(cygpath -m "$V")
export PATH="$V/bin:$W:$PATH"
B="$P/wbuild-vk"
# BUILD_SHARED_LIBS is required by GGML_BACKEND_DL; do NOT add -static-libgcc (ld: multiple definition of _Unwind_Resume)
cmake -S "$P/dl/src/whisper.cpp" -B "$B" -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_PREFIX_PATH="$VW" \
  -DBUILD_SHARED_LIBS=ON -DGGML_CCACHE=OFF -DGGML_VULKAN=ON -DGGML_BACKEND_DL=ON -DGGML_CPU_ALL_VARIANTS=ON -DGGML_NATIVE=OFF \
  -DWHISPER_BUILD_TESTS=OFF -DWHISPER_SDL2=OFF -DWHISPER_CURL=OFF \
  -DVulkan_INCLUDE_DIR="$VW/include" -DVulkan_LIBRARY="$VW/lib/libvulkan-1.dll.a" -DVulkan_GLSLC_EXECUTABLE="$VW/bin/glslc.exe"
cmake --build "$B" -j 8 --target whisper-server whisper-cli
taskkill //F //IM whisper-server.exe 2>/dev/null || true
G=app/bin/runtime/gpu; mkdir -p "$G"
cp "$B"/bin/whisper-server.exe "$B"/bin/whisper-cli.exe "$B"/bin/libwhisper.dll "$B"/bin/ggml*.dll "$G/"
for d in libgcc_s_seh-1 libgomp-1 libstdc++-6 libwinpthread-1 libdl; do cp "$W/$d.dll" "$G/"; done   # gcc runtime
ls -l "$G" | head -40
