#pragma once

#include <optional>

#include "InputTopology.h"

std::optional<RuntimeTopology> makeRp2040StandaloneDebugTopology(
    const BoardProfile &profile);
