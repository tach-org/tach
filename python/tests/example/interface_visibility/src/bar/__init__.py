from foo import Foo


class Bar:
    """Bar is allowed to import Foo (in visibility list)."""
    def __init__(self):
        self.foo = Foo()
