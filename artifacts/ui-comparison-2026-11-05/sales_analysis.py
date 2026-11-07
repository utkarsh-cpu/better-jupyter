import marimo

app = marimo.App(width="medium")

@app.cell
def _():
    import marimo as mo
    mo.md('# Sales analysis\nCompare a small dataset, inspect the result, and diagnose an error.')
    return

@app.cell
def _():
    sales = [120, 180, 150, 210]
    total = sum(sales)
    print(f"Total sales: {total}")
    print(f"Average: {total / len(sales):.1f}")
    return

@app.cell
def _():
    1 / 0
    return

if __name__ == "__main__":
    app.run()
