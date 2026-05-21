from pkg.api import UsedClass, exported_by_all, used_function
from pkg.service import USED_VALUE as imported_value
import pkg.dynamic
import pkg.public_api
import pkg.service as service_alias
from pkg import service as service_module

used_function()
UsedClass()
exported_by_all()
service_alias.alias_target()
service_module.module_attr_target()
print(imported_value)
