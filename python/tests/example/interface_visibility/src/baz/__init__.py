from foo import Foo
from domain import DomainFoo

class Baz:
    """Baz should NOT be allowed to import Foo or Domain (not in visibility list)."""
    def __init__(self):
        self.foo = Foo()
        self.domain = DomainFoo()
