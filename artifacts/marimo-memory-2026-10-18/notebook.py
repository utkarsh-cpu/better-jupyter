import marimo

__generated_with = "0.25.1"
app = marimo.App()


@app.cell
def _():
    import marimo as mo
    mo.md("# Memory benchmark\nA small idle notebook.")
    return


@app.cell
def _():
    print(1 + 2)
    return


if __name__ == "__main__":
    app.run()
