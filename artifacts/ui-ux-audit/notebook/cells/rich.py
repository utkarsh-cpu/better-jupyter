import pandas as pd
import matplotlib.pyplot as plt
from io import BytesIO

display(pd.DataFrame({"Quarter": ["Q1", "Q2", "Q3", "Q4"], "Revenue": [120, 185, 160, 210]}))
fig, ax = plt.subplots(figsize=(5, 2))
ax.bar(["Q1", "Q2", "Q3", "Q4"], [120, 185, 160, 210], color="#4f46e5")
ax.set_title("Quarterly revenue")
buf = BytesIO()
fig.savefig(buf, format="png", bbox_inches="tight")
plt.close(fig)
class AuditPlot:
    def _repr_png_(self):
        return buf.getvalue()
display(AuditPlot())
