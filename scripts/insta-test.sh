#!/usr/bin/env bash
echo "🔍 Running snapshot tests..."
if cargo insta test --no-quiet >/dev/null 2>&1; then
    echo "✅ All snapshots are up to date!"
else
    echo "📝 Snapshots need review. Use 'just insta-review' to review changes."
fi
