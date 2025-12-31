from foo import Foo


class Baz:
    """Baz should NOT be allowed to import Foo (not in visibility list)."""
    def __init__(self):
        self.foo = Foo()
