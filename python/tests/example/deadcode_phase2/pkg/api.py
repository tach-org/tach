from .service import USED_VALUE, helper

__all__ = ["exported_by_all"]


def public_api(func):
    return func


def used_function():
    def nested_unused():
        return "nested"

    return helper(USED_VALUE)


def unused_function():
    return "dead"


class UsedClass:
    def method(self):
        return helper(USED_VALUE)


class UnusedClass:
    def method(self):
        return "dead"


def exported_by_all():
    return "exported"


def configured_public():
    return "configured"


@public_api
def decorated_endpoint():
    return "decorated"
