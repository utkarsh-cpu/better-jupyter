import torch
from pathlib import Path
from recipe import build, advance
from kernelledger.gpu.notebook import register
torch.manual_seed(1309)
objects = build()
objects['model'].to('cuda:0')
training = register(objects=objects, source_root=Path.cwd(), files=['recipe.py'], recipe_source='recipe.py', factory='build', cursor={'batch':0})
advance(training)
