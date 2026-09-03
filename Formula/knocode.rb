class Knocode < Formula
  desc "AI Runtime for coding agents — local daemon with UDS+MessagePack, DBOS workflows"
  homepage "https://github.com/leonortega/knocode"
  version "0.4.0"
  url "https://github.com/leonortega/knocode/archive/v0.4.0.tar.gz"
  sha256 "REPLACE_WITH_SHA_AFTER_RELEASE"
  license "MIT"
  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release"
    bin.install "target/release/knocode"
    bin.install "target/release/knocode-daemon"
  end

  service do
    run [opt_bin/"knocode-daemon"]
    keep_alive true
    log_path var/"log/knocode.log"
    error_log_path var/"log/knocode.error.log"
  end

  test do
    assert_match "0.4.0", shell_output("#{bin}/knocode --version")
    assert_match "All critical checks", shell_output("#{bin}/knocode doctor")
  end
end
