class Coderun < Formula
  desc "AI Runtime for coding agents — local daemon with UDS+MessagePack, DBOS workflows"
  homepage "https://github.com/leonortega/coderun"
  version "0.4.0"
  url "https://github.com/leonortega/coderun/archive/v0.4.0.tar.gz"
  sha256 "REPLACE_WITH_SHA_AFTER_RELEASE"
  license "MIT"
  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release"
    bin.install "target/release/coderun"
    bin.install "target/release/coderun-daemon"
  end

  service do
    run [opt_bin/"coderun-daemon"]
    keep_alive true
    log_path var/"log/coderun.log"
    error_log_path var/"log/coderun.error.log"
  end

  test do
    assert_match "0.4.0", shell_output("#{bin}/coderun --version")
    assert_match "All critical checks", shell_output("#{bin}/coderun doctor")
  end
end
