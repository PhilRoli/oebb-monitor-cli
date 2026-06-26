class OebbMonitor < Formula
  desc "Terminal UI for live ÖBB departure and arrival data"
  homepage "https://github.com/philroli/oebb-monitor"
  version "0.2.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/philroli/oebb-monitor/releases/download/v0.2.0/oebb-monitor-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end

    on_intel do
      url "https://github.com/philroli/oebb-monitor/releases/download/v0.2.0/oebb-monitor-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    bin.install "oebb-monitor"
  end

  test do
    assert_match "oebb-monitor #{version}", shell_output("#{bin}/oebb-monitor --version")
  end
end
