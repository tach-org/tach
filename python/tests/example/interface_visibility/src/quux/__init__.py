from qux import Qux

class Quux:
    """Quux should NOT be able to import Qux (empty visibility list)."""
    def __init__(self):
        self.qux = Qux()
