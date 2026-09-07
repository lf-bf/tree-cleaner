# Homebrew formula for tree-cleaner.
#
# Until the first tagged release exists, install from the repository head:
#   brew tap lf-bf/tree-cleaner git@github.com:lf-bf/tree-cleaner.git
#   brew install --HEAD lf-bf/tree-cleaner/tree-cleaner
#
# After tagging a release (git tag v0.2.0 && git push --tags), fill `url` and `sha256`
# with the tarball of that tag (`shasum -a 256 v0.2.0.tar.gz`) and drop `--HEAD`.
class TreeCleaner < Formula
  desc "Interactive terminal explorer and cleaner for disk usage"
  homepage "https://github.com/lf-bf/tree-cleaner"
  url "https://github.com/lf-bf/tree-cleaner/archive/refs/tags/v0.2.0.tar.gz"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  license "MIT"
  head "git@github.com:lf-bf/tree-cleaner.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "tree-cleaner", shell_output("#{bin}/tree-cleaner --version")
    system bin/"tree-cleaner", "categories"
  end
end
