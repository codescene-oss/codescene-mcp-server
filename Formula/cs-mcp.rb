# typed: false
# frozen_string_literal: true

class CsMcp < Formula
  desc "MCP Server exposing Code Health analysis as AI-friendly tools"
  homepage "https://github.com/codescene-oss/codescene-mcp-server"
  version "1.5.11"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-macos-aarch64.zip"
      sha256 "ceb5edb700e216fb370e35a6ecad161ab21bb8630b86db6d4107c38bce7b27b5"

      define_method(:install) do
        bin.install "cs-mcp-macos-aarch64" => "cs-mcp"
      end
    end

    on_intel do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-macos-amd64.zip"
      sha256 "62d6c1c45325ee8c1c8d0c0ee5318ad1d9abfc53b3b82ed8fb33c1389674fc42"

      define_method(:install) do
        bin.install "cs-mcp-macos-amd64" => "cs-mcp"
      end
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-linux-aarch64.zip"
      sha256 "cdd80a595f700c4e4e0d160c3424d06c51846ef1a2cf754d2570256b76c78b37"

      define_method(:install) do
        bin.install "cs-mcp-linux-aarch64" => "cs-mcp"
      end
    end

    on_intel do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-linux-amd64.zip"
      sha256 "0fcf7c37cf3374fd3fe1e46d23092b569e19c443b627fa8d5bc70c8f31dbb095"

      define_method(:install) do
        bin.install "cs-mcp-linux-amd64" => "cs-mcp"
      end
    end
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/cs-mcp --version 2>&1", 2)
  end
end
