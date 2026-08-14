#include "Handshake.h"

namespace {
bool containsAsciiWhitespace(std::string_view value) {
  for (const char character : value) {
    switch (character) {
      case ' ':
      case '\t':
      case '\n':
      case '\v':
      case '\f':
      case '\r':
        return true;
    }
  }
  return false;
}
}  // namespace

std::string formatHello(const BoardProfile &profile,
                        std::string_view firmwareBuildId) {
  if (firmwareBuildId.empty() || containsAsciiWhitespace(firmwareBuildId)) {
    return {};
  }

  std::string line = "HELLO 10 ";
  line += profile.controllerFamilyId;
  line += ' ';
  line += profile.boardProfileId;
  line += ' ';
  line += firmwareBuildId;
  line += ' ';
  line += '-';
  line += ' ';
  line += std::to_string(profile.safePinCount);
  for (std::size_t index = 0; index < profile.safePinCount; ++index) {
    line += ' ';
    line += std::to_string(profile.safePins[index]);
  }
  line += '\n';
  return line;
}

std::string formatHello(const BoardProfile &profile,
                        std::string_view firmwareBuildId,
                        std::string_view productVersionId) {
  if (firmwareBuildId.empty() || containsAsciiWhitespace(firmwareBuildId) ||
      productVersionId.empty() || containsAsciiWhitespace(productVersionId)) {
    return {};
  }

  std::string line = "HELLO 10 ";
  line += profile.controllerFamilyId;
  line += ' ';
  line += profile.boardProfileId;
  line += ' ';
  line += firmwareBuildId;
  line += ' ';
  line += productVersionId;
  line += ' ';
  line += std::to_string(profile.safePinCount);
  for (std::size_t index = 0; index < profile.safePinCount; ++index) {
    line += ' ';
    line += std::to_string(profile.safePins[index]);
  }
  line += '\n';
  return line;
}
