#pragma once

#include <optional>

#include "InputTopology.h"

std::optional<RuntimeTopology> makeRp2040StandaloneDebugTopology(
    const BoardProfile &profile);
bool acceptsRp2040StandaloneHostTopology(const RuntimeTopology &topology);
