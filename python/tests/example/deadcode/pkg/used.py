from typing import TYPE_CHECKING

from pkg.helper import helper

if TYPE_CHECKING:
    from pkg.models import Model


def run() -> "Model | None":
    helper()
    return None
