# Homebrew formula for codescope.
#
# Ships as a Homebrew tap so users on macOS + Linux can install
# with a single command instead of piping curl. Binaries come
# from the GitHub release archive produced by release.yml — we
# don't compile from source here because (a) the full build
# takes 15+ minutes on a laptop and (b) ort-sys has no prebuilt
# x86_64-apple-darwin wheels, which would fail at brew install
# time on Intel Macs. Source build is still available via
# `cargo install --path crates/cli` for developers.
#
# The tap is published at onur-gokyildiz-bhi/homebrew-codescope;
# after bumping `VERSION` here, tag a release and update `sha256`
# values with the new archive checksums.
class Codescope < Formula
  desc "Graph-first code intelligence engine for AI agents"
  homepage "https://github.com/onur-gokyildiz-bhi/codescope"
  version "0.7.10"
  license "MIT"

  # Bundled surreal server — needed by the R4 supervisor. We drop
  # it into `~/.codescope/bin/` on install (see `post_install`)
  # instead of `bin/` because it's an internal dependency, not a
  # user-facing CLI.
  SURREAL_VERSION = "v3.0.5"

  on_macos do
    on_arm do
      url "https://github.com/onur-gokyildiz-bhi/codescope/releases/download/v#{version}/codescope-v#{version}-aarch64-apple-darwin.tar.gz"
      # sha256 filled in per release by release CI — placeholder
      # so `brew audit` has a shape to validate.
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    # Intel Mac: no prebuilt — ort-sys lacks x86_64-apple-darwin
    # wheels. Direct users at `cargo install`.
  end

  on_linux do
    on_intel do
      url "https://github.com/onur-gokyildiz-bhi/codescope/releases/download/v#{version}/codescope-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
    on_arm do
      url "https://github.com/onur-gokyildiz-bhi/codescope/releases/download/v#{version}/codescope-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  def install
    # Main CLI + adjacent binaries land on `bin/` so they're on
    # PATH. surreal goes into the user's `~/.codescope/bin/`
    # directory on post-install (see below) so it doesn't show up
    # as a top-level brew binary the user didn't ask for.
    bin.install "codescope"
    bin.install "codescope-mcp"
    bin.install "codescope-web" if File.exist?("codescope-web")
    bin.install "codescope-lsp" if File.exist?("codescope-lsp")
  end

  def post_install
    # Copy bundled surreal into the location the supervisor looks
    # first (`~/.codescope/bin/surreal`). Skipped silently if the
    # archive didn't include surreal (older releases).
    src = "#{buildpath}/surreal"
    dst_dir = "#{Dir.home}/.codescope/bin"
    FileUtils.mkdir_p(dst_dir)
    if File.exist?(src) && !File.exist?("#{dst_dir}/surreal")
      FileUtils.cp(src, "#{dst_dir}/surreal")
      FileUtils.chmod(0755, "#{dst_dir}/surreal")
    end
  end

  test do
    # `--version` is the cheapest smoke test. If this passes the
    # binary loaded OpenSSL and clap fine.
    system "#{bin}/codescope", "--version"
  end
end
