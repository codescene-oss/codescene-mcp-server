# typed: false
# frozen_string_literal: true

class CsMcp < Formula
  desc "MCP Server exposing Code Health analysis as AI-friendly tools"
  homepage "https://github.com/codescene-oss/codescene-mcp-server"
  version "1.0.2"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-macos-aarch64.zip"
      sha256 "7330660010ea37b035f0baf477a306fa332d3e5e4c07b665be8010b87f07b84d"

      define_method(:install) do
        bin.install "cs-mcp-macos-aarch64" => "cs-mcp"
      end
    end

    on_intel do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-macos-amd64.zip"
      sha256 "a53fee7b77320015f3e918e83a190f10d05712cd5861f30ddc32d381e37f780b"

      define_method(:install) do
        bin.install "cs-mcp-macos-amd64" => "cs-mcp"
      end
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-linux-aarch64.zip"
      sha256 "7330584002074d2826045585e1c5f0648057e6dde71b4c17ca1ded9d80f0adb6"

      define_method(:install) do
        bin.install "cs-mcp-linux-aarch64" => "cs-mcp"
      end
    end

    on_intel do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-linux-amd64.zip"
      sha256 "4a4ea2d5d58893dad99378f3792708f8116a16b7b7fdf36c504cde8702a86513"

      define_method(:install) do
        bin.install "cs-mcp-linux-amd64" => "cs-mcp"
      end
    end
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/cs-mcp --version 2>&1", 2)
  end
end
