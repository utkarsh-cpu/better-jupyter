"""Generate the versioned scorecard without estimating untested criteria."""
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1]
data = json.loads((root / "docs/desktop/ux-scorecard.json").read_text(encoding='utf-8'))
assert abs(sum(c["weight"] for c in data["categories"]) - 100) < 1e-8
assert len(data["categories"]) == 18
assert all(c["requirements"] for cat in data["categories"] for c in cat["criteria"])
assert {f"J{i}" for i in range(1, 9)} <= {j for cat in data["categories"] for c in cat["criteria"] for j in c["journeys"]}
lines = ["# UX scorecard v1", "", "Frontend plan 3. Original audit evidence remains unchanged. This build is not qualified.", "", "| Category | Weight | Rating | Coverage |", "| --- | ---: | ---: | ---: |"]
points = rated_weight = coverage = 0
for category in data["categories"]:
    ratings = [c["score"] for c in category["criteria"] if c["score"] is not None]
    assert all(type(v) in (int, float) and 0 <= v <= 5 for v in ratings)
    weight = category["weight"]
    fraction = len(ratings) / len(category["criteria"])
    coverage += weight * fraction
    if ratings:
        average = sum(ratings) / len(ratings)
        points += weight * average / 5
        rated_weight += weight
        label = f"{average:.2f}/5"
    else:
        label = "NT"
    lines.append(f"| {category['name']} | {weight:g} | {label} | {fraction:.0%} |")
score = f"{points * 100 / rated_weight:.1f}/100" if rated_weight else "Not scored"
lines += ["", f"Score: {score}. Weighted evidence coverage: {coverage:.1f}%.", "", "Release requires at least 80 points, 90% coverage, no open P1 finding, and directly verified keyboard, screen-reader and recovery journeys. Formative and summative studies require 5-8 participants.", "", "Update ux-scorecard.json with build, environment, scores and evidence at each phase exit; archive earlier snapshots. NT is never a pass or a failure."]
(root / "docs/desktop/ux-scorecard.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"18 categories, 72 criteria, weights 100. {score}; coverage {coverage:.1f}%")
