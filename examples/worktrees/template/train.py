# These globals come from the load cell or a restored kernel checkpoint.
# ruff: noqa: F821


def train(epochs):
    global training_steps
    for _ in range(epochs):
        errors = [model["slope"] * x + model["intercept"] - y for x, y in samples]
        slope_gradient = 2 * sum(error * row[0] for error, row in zip(errors, samples))
        intercept_gradient = 2 * sum(errors)
        model["slope"] -= config["learning_rate"] * slope_gradient / len(samples)
        model["intercept"] -= config["learning_rate"] * intercept_gradient / len(samples)
        training_steps += 1
    metrics["mse"] = sum(
        (model["slope"] * x + model["intercept"] - y) ** 2 for x, y in samples
    ) / len(samples)


train(config["epochs"])
print({"steps": training_steps, "model": model, "metrics": metrics})
