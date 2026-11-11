class HTML:
    """Same display contract as IPython.display.HTML."""

    def __init__(self, data):
        self.data = data

    def _repr_html_(self):
        return self.data

print("HTML helper ready")
