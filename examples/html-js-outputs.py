"""Create examples/html-js-outputs with every HTML+JS output kind."""
from __future__ import annotations

import json
from pathlib import Path

from kernelledger.notebook import Notebook

ROOT = Path(__file__).resolve().parent / "html-js-outputs"

CELLS: list[tuple[str, str, str]] = [
    (
        "markdown",
        "intro",
        """# HTML+JS output examples

Each code cell records a different `text/html` shape. Native pandas tables stay
tables. Interactive pages need **Settings → Allow interactive HTML output**, then
**Output tools → Render HTML**. The webview has no network, so CDN scripts stay blank
unless the HTML is self-contained.
""",
    ),
    (
        "code",
        "html-helper",
        '''class HTML:
    """Same display contract as IPython.display.HTML."""

    def __init__(self, data):
        self.data = data

    def _repr_html_(self):
        return self.data

print("HTML helper ready")
''',
    ),
    (
        "code",
        "pandas-table",
        """import pandas as pd

df = pd.DataFrame({"name": ["Ada", "Bob", "Cyd"], "score": [3, 9, 6]})
df
""",
    ),
    (
        "code",
        "pandas-styler",
        """df.style.highlight_max(axis=0)
""",
    ),
    (
        "code",
        "custom-page",
        """display(HTML("<div style='padding:12px;border:1px solid #888'><strong>Custom page</strong><p>No table, no script.</p></div>"))
""",
    ),
    (
        "code",
        "inline-js",
        """display(HTML('''
<div id="box" style="padding:12px;background:#102a43;color:#fff;cursor:pointer">Click me</div>
<script>
document.getElementById("box").addEventListener("click", function () {
  this.textContent = "Inline script ran";
});
</script>
'''))
""",
    ),
    (
        "code",
        "table-onclick",
        """display(HTML('''
<table>
  <tr><th>Name</th><th>Action</th></tr>
  <tr><td>Ada</td><td onclick="this.textContent='clicked'">click</td></tr>
</table>
'''))
""",
    ),
    (
        "code",
        "iframe",
        """display(HTML("<iframe srcdoc='<p>Hello from an iframe</p>' style='width:100%;height:120px'></iframe>"))
""",
    ),
    (
        "code",
        "object-embed",
        """display(HTML("<object data='about:blank' style='height:80px;width:100%'></object><embed src='about:blank' style='height:80px;width:100%'>"))
""",
    ),
    (
        "code",
        "javascript-href",
        """display(HTML("<a href='javascript:void(0)'>javascript: link</a>"))
""",
    ),
    (
        "code",
        "svg-html",
        """display(HTML("<svg xmlns='http://www.w3.org/2000/svg' width='160' height='80'><circle cx='40' cy='40' r='30' fill='#3b82f6'/><text x='80' y='45' fill='#111'>SVG</text></svg>"))
""",
    ),
    (
        "code",
        "plotly-fig",
        """import plotly.express as px

fig = px.bar(x=["a", "b", "c"], y=[1, 3, 2], title="Plotly (MIME bundle)")
fig
""",
    ),
    (
        "code",
        "plotly-cdn",
        """display(HTML('''
<script src="https://cdn.plot.ly/plotly-2.27.0.min.js"></script>
<div>CDN Plotly: this stays blank after Render HTML because the webview has no network.</div>
'''))
""",
    ),
    (
        "code",
        "bokeh-fig",
        """from bokeh.embed import file_html
from bokeh.plotting import figure
from bokeh.resources import CDN

plot = figure(width=360, height=220, title="Bokeh")
plot.line([1, 2, 3], [1, 4, 9], line_width=2)
display(HTML(file_html(plot, CDN, "Bokeh")))
""",
    ),
    (
        "code",
        "altair-chart",
        """import altair as alt
import pandas as pd

chart = (
    alt.Chart(pd.DataFrame({"x": ["a", "b", "c"], "y": [1, 3, 2]}))
    .mark_bar()
    .encode(x="x", y="y")
    .properties(title="Altair / Vega")
)
chart
""",
    ),
    (
        "code",
        "oversize-html",
        """display(HTML("<div><script>" + ("x=1;" * 25000) + "</script>oversize</div>"))
""",
    ),
]


def main() -> None:
    if (ROOT / "notebook.json").exists():
        raise SystemExit(f"{ROOT} already exists; delete it to recreate")
    notebook = Notebook(ROOT)
    notebook.init()
    for kind, cid, source in CELLS:
        notebook.add(kind, source, cid)
    print(f"created {ROOT} with {len(CELLS)} cells")


if __name__ == "__main__":
    main()
