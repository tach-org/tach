USED_VALUE = 7
UNUSED_VALUE = 0


def helper(value):
    return value


def alias_target():
    return helper(USED_VALUE)


def module_attr_target():
    return helper(USED_VALUE)


def unused_service():
    return "dead"
