# Homebrew formula for tree-cleaner.
#
#   brew tap lf-bf/tree-cleaner ssh://git@github.com/lf-bf/tree-cleaner.git
#   brew install lf-bf/tree-cleaner/tree-cleaner
#   brew upgrade tree-cleaner        # after `brew update`
#
# The repository is private, so the formula builds from the release tag over SSH instead of
# downloading a tarball. Homebrew reads the version from the tag; `tag` is bumped by
# semantic-release on every release (scripts/release/prepare.sh), do not edit it by hand.
# `brew audit` asks for a `revision:` next to the tag; it cannot be known when the release
# commit is prepared, and installs work without it.
class TreeCleaner < Formula
  desc "Interactive terminal explorer and cleaner for disk usage"
  homepage "https://github.com/lf-bf/tree-cleaner"
  url "ssh://git@github.com/lf-bf/tree-cleaner.git", tag: "v0.2.0"
  license "MIT"
  head "ssh://git@github.com/lf-bf/tree-cleaner.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "tree-cleaner #{version}", shell_output("#{bin}/tree-cleaner --version")
    system bin/"tree-cleaner", "categories"
  end
end
