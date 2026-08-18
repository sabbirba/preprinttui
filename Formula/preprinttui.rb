class Preprinttui < Formula
  desc "Interactive Terminal User Interface for PreConnect printer"
  homepage "https://preconnect.app"
  version "0.1.2"
  license "GPL-3.0-only"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/sabbirba/preprinttui/releases/download/v#{version}/preprinttui-macos-arm64.tar.gz"
      sha256 "9f9c7682cb1b6d669edb9c7cebe2e4b5425d7a500ee69d425d663b9ad7466348"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/sabbirba/preprinttui/releases/download/v#{version}/preprinttui-linux-musl-x86_64.tar.gz"
      sha256 "e754067d1f63232ab28a375974547f395766efa433a43fa2b72fc86c7b7b3497"
    end
  end

  def install
    bin.install "preprinttui"
  end

  test do
    assert_match "preprinttui", shell_output("#{bin}/preprinttui --version 2>&1", 0)
  end
end
