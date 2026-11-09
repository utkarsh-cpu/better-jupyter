from bokeh.embed import file_html
from bokeh.plotting import figure
from bokeh.resources import CDN

plot = figure(width=360, height=220, title="Bokeh")
plot.line([1, 2, 3], [1, 4, 9], line_width=2)
display(HTML(file_html(plot, CDN, "Bokeh")))
