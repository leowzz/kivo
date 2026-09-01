#pragma once

#include <string>
#include <string_view>

#include "BoardProfile.h"

std::string formatHello(const BoardProfile &profile,
                        std::string_view firmwareBuildId);
std::string formatHello(const BoardProfile &profile,
                        std::string_view firmwareBuildId,
                        std::string_view productVersionId);
