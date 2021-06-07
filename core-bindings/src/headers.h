#include <rometadataresolution.h>
#include <roparameterizediid.h>
#include <stdint.h>
# define NDEBUG
#include <cassert>
#include <array>
#include <iostream>
#include <string>
#include <hstring.h>
#include <sstream>
#include <winstring.h>
const size_t MAX_IDENTIFIER_LENGTH{ 511 };
using identifier = std::array<wchar_t, MAX_IDENTIFIER_LENGTH + 1>;

typedef struct _locator Locator;