"""Captured factory and training step for the notebook checkpoint example."""

import torch


def build():
    model = torch.nn.Sequential(torch.nn.Linear(4, 8), torch.nn.Dropout(0.2), torch.nn.Linear(8, 1))
    optimizer = torch.optim.Adam(model.parameters(), lr=0.01)
    return {"model": model, "optimizer": optimizer}


def advance(training):
    with training.training_step(cursor={"batch": training.status["step"] + 1}) as objects:
        model, optimizer = objects["model"], objects["optimizer"]
        device = next(model.parameters()).device
        inputs = torch.arange(16, device=device, dtype=torch.float32).reshape(4, 4) / 16
        optimizer.zero_grad()
        loss = model(inputs).square().mean()
        loss.backward()
        optimizer.step()
    return {"loss": loss.item(), "weights": [p.detach().cpu().tolist() for p in model.parameters()]}
