import json
import pathlib

config = json.loads(pathlib.Path("parameters.json").read_text())
samples = json.loads(pathlib.Path(config["dataset"]).read_text())
model = {"slope": 0.0, "intercept": 0.0}
model_alias = model
training_steps = 0
metrics = {}
