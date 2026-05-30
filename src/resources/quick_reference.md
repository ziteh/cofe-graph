# Quick Reference

| Question                                             | Tool                                         |
| ---------------------------------------------------- | -------------------------------------------- |
| Where is function / type / macro X defined?          | `search`                                     |
| Who calls function X?                                | `traverse direction="callers"`               |
| What does function X call?                           | `traverse direction="callees"`               |
| Show the call path from X to Y                       | `get_path`                                   |
| Show me the source of function X                     | `get_source`                                 |
| What functions are defined in `foo.c`?               | `find_functions_in_file`                     |
| What global variables does `foo.c` declare?          | `get_globals`                                |
| Which functions reference global variable X?         | `find_users kind="global"`                   |
| Which functions use struct / type X?                 | `find_users kind="type"`                     |
| What does `foo.c` `#include`?                        | `includes direction="outbound"`              |
| Which files include `bar.h`?                         | `includes direction="inbound"`               |
| Find functions that are never called                 | `find_dead_code`                             |
| Source files changed — rebuild the index             | `index_project`                              |
| Annotate a file or symbol (commit-aware)             | `annotate`                                   |
| Define a logical module grouping                     | `annotate_module`                            |
| Read annotation for a file / symbol / module         | `get_annotations`                            |
| What files / functions / globals haven't been annotated? | `list_unannotated`                       |
