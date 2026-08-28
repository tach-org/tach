def __getattr__(name):
    return globals()[name]


def dynamic_dead():
    return "hidden by dynamic module suppression"
