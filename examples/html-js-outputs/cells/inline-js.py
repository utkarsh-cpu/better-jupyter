display(HTML('''
<div id="box" style="padding:12px;background:#102a43;color:#fff;cursor:pointer">Click me</div>
<script>
document.getElementById("box").addEventListener("click", function () {
  this.textContent = "Inline script ran";
});
</script>
'''))
