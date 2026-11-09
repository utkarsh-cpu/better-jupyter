import altair as alt
import pandas as pd

chart = (
    alt.Chart(pd.DataFrame({"x": ["a", "b", "c"], "y": [1, 3, 2]}))
    .mark_bar()
    .encode(x="x", y="y")
    .properties(title="Altair / Vega")
)
chart
