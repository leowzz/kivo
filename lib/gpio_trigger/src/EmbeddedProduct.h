#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>

#include "InputTopology.h"

#if __has_include(<KivoProductGenerated.h>)
#include <KivoProductGenerated.h>
#else
inline constexpr char kKivoProductVersionId[] = "-";
inline constexpr char kKivoProductDefinitionSha256[] = "-";
inline constexpr std::uint8_t kKivoProductDefinition[] = {0};
inline constexpr std::size_t kKivoProductDefinitionSize = 0;

inline std::optional<RuntimeTopology> makeEmbeddedProductTopology(
    const BoardProfile &) {
  return std::nullopt;
}
#endif
