#!/bin/bash
set -e

NEW_VERSION=$1

if [ -z "$NEW_VERSION" ]; then
  echo "Usage: ./scripts/bump-version.sh <version>"
  echo "Example: ./scripts/bump-version.sh 0.2.0"
  exit 1
fi

echo "Bumping version to $NEW_VERSION..."

# Update package.json
sed -i '' "s/\"version\": \".*\"/\"version\": \"$NEW_VERSION\"/" package.json

# Update Cargo.toml (first occurrence only - the package version)
sed -i '' "0,/^version = \".*\"/s//version = \"$NEW_VERSION\"/" src-tauri/Cargo.toml

# Update tauri.conf.json
sed -i '' "s/\"version\": \".*\"/\"version\": \"$NEW_VERSION\"/" src-tauri/tauri.conf.json

echo "Version bumped to $NEW_VERSION"
echo ""
echo "Next steps:"
echo "  1. Commit changes: git add -A && git commit -m 'chore: bump version to $NEW_VERSION'"
echo "  2. Create tag: git tag v$NEW_VERSION"
echo "  3. Push: git push && git push --tags"
echo ""
echo "GitHub Actions will automatically build and release when the tag is pushed."
