"""Preserve reviewer outputs, verify arithmetic, and build an offline score chart."""
import hashlib
import json
import shutil
from decimal import Decimal, ROUND_HALF_UP
from pathlib import Path

OUT = Path(__file__).resolve().parent
sources = {
    "grok": Path("C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-2cca49b1/artifacts/assistant-review-2026-11-06"),
    "sol": Path("C:/Users/UTKARSH KAUSHIK/.t3/worktrees/KernelLedger-v6/t3-8de56f2d/artifacts/assistant-review-2026-11-06"),
}
reviews, provenance = {}, {}
for reviewer, directory in sources.items():
    for suffix in ("review.md", "scores.json"):
        name = f"{reviewer}-{suffix}"
        content = (directory / name).read_bytes()
        target = OUT / name
        if target.exists():
            assert target.read_bytes() == content, f"Preserve existing differing report: {name}"
        else:
            shutil.copyfile(directory / name, target)
        provenance[name] = {"source": str(directory / name), "sha256": hashlib.sha256(content).hexdigest()}
    original = json.loads((OUT / f"{reviewer}-scores.json").read_text(encoding="utf-8"))
    groups = {}
    for group in ("ui", "backend"):
        source_key = "ui_ux" if reviewer == "sol" and group == "ui" else group
        data = original[source_key]
        dims = [
            {"name": d.get("name", d.get("dimension")), "score": d["score"], "confidence": d["confidence"], "rationale": d["rationale"]}
            for d in data["dimensions"]
        ]
        scores = [Decimal(str(d["score"])) for d in dims if d["score"] is not None]
        total = sum(scores)
        exact = total / len(scores)
        display = exact.quantize(Decimal("0.1"), rounding=ROUND_HALF_UP)
        assert len(scores) == 10
        assert total == Decimal(str(data["score_sum" if reviewer == "sol" else "sum"]))
        assert exact == Decimal(str(data["mean_exact" if reviewer == "sol" else "mean_unrounded"]))
        assert display == Decimal(str(data["mean_display"]))
        groups[group] = {"dimensions": dims, "sum": str(total), "count": len(scores), "mean_exact": str(exact), "mean_display": str(display)}
    reviews[reviewer] = {"label": "Grok 4.7" if reviewer == "grok" else "GPT 6.1 Sol", "effort": "xhigh", **groups}

for group in ("ui", "backend"):
    assert len(reviews["grok"][group]["dimensions"]) == len(reviews["sol"][group]["dimensions"]) == 10

summary = {
    "reviews": reviews,
    "provenance": provenance,
    "interpretation": "Independent subjective review ratings, medium overall confidence. One saved fixture; no pooled consensus mean or demonstrated P0. Scored dimensions are not full behavioral coverage.",
    "rounding": "Exact decimal means, displayed half-up to one decimal.",
}
(OUT / "comparison.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
(OUT / "provenance.json").write_text(json.dumps(provenance, indent=2), encoding="utf-8")

rows = []
for group, label in (("ui", "UI/UX"), ("backend", "Backend")):
    rows += [f"### {label}", "", "| Dimension | Grok 4.7 | GPT 6.1 Sol |", "|---|---:|---:|"]
    for a, b in zip(reviews["grok"][group]["dimensions"], reviews["sol"][group]["dimensions"]):
        rows.append(f"| {a['name']} | {a['score']:.1f} | {b['score']:.1f} |")
    rows.append(f"| **Mean / 10** | **{reviews['grok'][group]['mean_display']}** | **{reviews['sol'][group]['mean_display']}** |")
    rows += ["", f"Exact means: Grok {reviews['grok'][group]['sum']} / 10 = {reviews['grok'][group]['mean_exact']}; Sol {reviews['sol'][group]['sum']} / 10 = {reviews['sol'][group]['mean_exact']}. Half-up display; all ten dimensions scored.", ""]
(OUT / "score-tables.md").write_text("\n".join(rows), encoding="utf-8")

html = '''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<style>
:root{--grok:var(--chart-1,royalblue);--sol:var(--chart-2,darkorange);font-family:var(--font-sans,system-ui);color:var(--foreground,CanvasText)}
html,body{margin:0;background:transparent}*{box-sizing:border-box}button{font:inherit;color:inherit}
.top{display:flex;flex-wrap:wrap;align-items:center;justify-content:space-between;gap:12px;margin-bottom:12px}
.tabs{display:flex;gap:6px}.tabs button{padding:8px 14px;border:1px solid var(--border,ButtonBorder);border-radius:var(--radius,8px);background:transparent;cursor:pointer}
.tabs button[aria-selected=true]{background:var(--secondary,ButtonFace);font-weight:650;border-color:var(--ring,Highlight)}
button:focus-visible{outline:2px solid var(--ring,Highlight);outline-offset:3px}
.legend{display:flex;gap:16px;font-size:13px;flex-wrap:wrap}.legend span{display:flex;gap:6px;align-items:center}.dot{width:10px;height:10px;border-radius:50%;display:inline-block}
.means{display:flex;gap:18px;flex-wrap:wrap;margin:10px 0 12px;font-size:14px}.means strong{font-size:19px;font-variant-numeric:tabular-nums}
.axis{display:grid;grid-template-columns:minmax(110px,33%) minmax(0,1fr);gap:12px;margin-bottom:4px;font-size:11px;color:var(--muted-foreground,GrayText)}
.scale{display:flex;justify-content:space-between;margin-left:36px;margin-right:29px}
.chart{height:680px;display:grid;grid-template-rows:repeat(10,68px)}
.row{display:grid;grid-template-columns:minmax(110px,33%) minmax(0,1fr);gap:12px;align-items:center;border-top:1px solid var(--border,ButtonBorder)}
.label{font-size:13px;line-height:1.3;overflow-wrap:anywhere}.pair{display:grid;gap:6px}
.barline{display:grid;grid-template-columns:30px minmax(0,1fr) 25px;gap:6px;align-items:center;font-size:11px}
.track{height:14px;background:var(--secondary,ButtonFace);border-radius:3px;overflow:hidden}.bar{height:14px;border-radius:3px;background:var(--grok)}
.value{text-align:right;font-variant-numeric:tabular-nums;font-weight:650;font-size:12px}
.note{font-size:12px;color:var(--muted-foreground,GrayText);line-height:1.45;margin:12px 0 0}
@media(max-width:450px){.label{font-size:12px}.top{align-items:flex-start}.legend{gap:12px;font-size:12px}.means{font-size:12px;gap:12px}.means strong{font-size:18px}.row,.axis{gap:8px;grid-template-columns:minmax(105px,35%) minmax(0,1fr)}}
</style></head><body>
<div class="top"><div class="tabs" role="tablist" aria-label="Review dimensions"><button id="ui-tab" role="tab" aria-selected="true" aria-controls="chart" data-group="ui">UI / UX</button><button id="backend-tab" role="tab" aria-selected="false" aria-controls="chart" data-group="backend">Backend</button></div>
<div class="legend"><span><i class="dot" style="background:var(--grok)" aria-hidden="true"></i>Grok 4.7 · xhigh</span><span><i class="dot" style="background:var(--sol)" aria-hidden="true"></i>GPT 6.1 Sol · xhigh</span></div></div>
<div id="means" class="means" aria-live="polite"></div><div class="axis"><span>Ratings / 10</span><div class="scale"><span>0</span><span>5</span><span>10</span></div></div>
<div id="chart" class="chart" role="tabpanel" aria-labelledby="ui-tab"></div>
<p class="note">Independent review judgments, with medium overall confidence. Both reviewed the same saved Sol UI and backend evidence. Ten dimensions per group; means use exact arithmetic and half-up rounding. One error fixture and one successful native run do not establish general reliability or accessibility.</p>
<script>
const reviews=__DATA__;
function draw(group){
 document.querySelectorAll('[data-group]').forEach(b=>b.setAttribute('aria-selected',String(b.dataset.group===group)));
 document.getElementById('chart').setAttribute('aria-labelledby',group+'-tab');
 document.getElementById('means').innerHTML=['grok','sol'].map(r=>'<span>'+reviews[r].label+' mean <strong>'+reviews[r][group].mean_display+'</strong> / 10</span>').join('');
 const chart=document.getElementById('chart');chart.replaceChildren();
 reviews.grok[group].dimensions.forEach((d,i)=>{
  const row=document.createElement('div');row.className='row';const label=document.createElement('div');label.className='label';label.textContent=(i+1)+'. '+d.name;row.append(label);
  const pair=document.createElement('div');pair.className='pair';
  ['grok','sol'].forEach(r=>{const x=reviews[r][group].dimensions[i];const line=document.createElement('div');line.className='barline';line.title=x.confidence+' confidence: '+x.rationale;
   const author=document.createElement('span');author.textContent=r==='grok'?'Grok':'Sol';line.append(author);
   const track=document.createElement('div');track.className='track';track.setAttribute('role','meter');track.setAttribute('aria-label',reviews[r].label+': '+d.name);track.setAttribute('aria-valuemin','0');track.setAttribute('aria-valuemax','10');track.setAttribute('aria-valuenow',String(x.score));
   const bar=document.createElement('div');bar.className='bar';bar.style.width=(x.score*10)+'%';bar.style.background='var(--'+r+')';track.append(bar);line.append(track);
   const value=document.createElement('span');value.className='value';value.textContent=x.score.toFixed(1);line.append(value);pair.append(line);
  });row.append(pair);chart.append(row);
 });
}
document.querySelectorAll('[data-group]').forEach(b=>b.addEventListener('click',()=>draw(b.dataset.group)));draw('ui');
</script></body></html>'''.replace('__DATA__', json.dumps(reviews).replace('</', '<\\/'))
(OUT / "comparison.html").write_text(html, encoding="utf-8")
print(json.dumps({r:{g:reviews[r][g]["mean_display"] for g in ("ui","backend")} for r in reviews}))
print("Preserved four reviewer outputs; verified 40 dimension scores and four exact means; wrote comparison.json, score-tables.md and comparison.html.")
