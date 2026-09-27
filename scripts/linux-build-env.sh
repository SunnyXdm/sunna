# Sourced by the scripts that build the Linux host: checks for the tools a
# release build needs, and says how to install any that are missing.

need_package() {  # need_package <what> <apt package> <pacman package> <dnf package>
  echo "$1 is missing. Install it with:" >&2
  if command -v apt-get >/dev/null; then echo "  sudo apt install $2" >&2
  elif command -v pacman >/dev/null; then echo "  sudo pacman -S $3" >&2
  elif command -v dnf >/dev/null; then echo "  sudo dnf install $4" >&2
  else echo "  your distribution's $2 package" >&2
  fi
  exit 1
}

check_build_tools() {
  command -v cargo >/dev/null || {
    echo "Rust is missing. Install it with:" >&2
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
    exit 1
  }
  command -v c++ >/dev/null || need_package "A C/C++ compiler" build-essential base-devel gcc-c++
  if [ "$(uname -m)" = x86_64 ]; then
    # OpenH264, the video encoder for hosts without a supported GPU, needs
    # nasm for its SIMD code. Without it the build quietly falls back to
    # plain C, which encodes 3-4x slower.
    command -v nasm >/dev/null || need_package "nasm (for a fast video encoder)" nasm nasm nasm
    # A build made before nasm was installed stays slow until it's redone.
    if grep -qs "not using any assembly" target/release/build/openh264-sys2-*/output; then
      echo "Rebuilding the video encoder with nasm…"
      cargo clean --release -p openh264-sys2 >/dev/null 2>&1 || true
    fi
  fi
}
