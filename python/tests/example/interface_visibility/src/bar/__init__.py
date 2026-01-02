from foo import Foo
from domain import DomainFoo

class Bar:
    """Bar is allowed to import Foo and Domain (in visibility list)."""
    def __init__(self):
        self.foo = Foo()
        self.domain = DomainFoo()
