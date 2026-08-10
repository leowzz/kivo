#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>

struct HelperCommand;

constexpr std::size_t kMaxDisplayRegions = 8;
constexpr std::size_t kMaxDisplayOps = 24;
constexpr std::size_t kMaxDisplayTextBytes = 48;
constexpr std::uint16_t kRemoteDisplayWidth = 128;
constexpr std::uint16_t kRemoteDisplayHeight = 32;
constexpr std::uint8_t kRemoteDisplayFontId = 0;

enum class DisplayMode : std::uint8_t { Full, Delta };
enum class DisplayResult : std::uint8_t { Accepted, Resync, Rejected };
enum class DisplayOperationKind : std::uint8_t { Clear, Text };

struct DisplayRect {
  std::uint16_t x;
  std::uint16_t y;
  std::uint16_t width;
  std::uint16_t height;
};

struct DisplayOperation {
  std::string text;
  std::uint16_t x = 0;
  std::uint16_t baselineY = 0;
  std::uint8_t slot = 0;
  std::uint8_t fontId = 0;
  DisplayOperationKind kind = DisplayOperationKind::Clear;
};

struct DisplayRegionState {
  std::uint8_t slot = 0;
  DisplayRect bounds{};
};

struct RemoteDisplayScene {
  std::uint32_t revision = 0;
  std::array<DisplayRegionState, kMaxDisplayRegions> regions{};
  std::size_t regionCount = 0;
  std::array<DisplayOperation, kMaxDisplayOps> operations{};
  std::size_t operationCount = 0;
};

struct RemoteDisplayCommit : RemoteDisplayScene {
  bool full = false;
  std::array<DisplayRect, kMaxDisplayRegions> dirtyBounds{};
  std::size_t dirtyCount = 0;
};

static_assert(sizeof(DisplayRegionState) <= 16,
              "display regions must remain compact metadata");
static_assert(sizeof(RemoteDisplayScene) <= 1200,
              "display scenes must own only 24 operations");
static_assert(sizeof(RemoteDisplayCommit) <= 1280,
              "display commits must own only 24 operations");

class RemoteDisplay {
 public:
  DisplayResult begin(std::uint32_t newRevision, std::uint32_t baseRevision,
                      DisplayMode mode);
  bool region(std::uint8_t slot, DisplayRect bounds);
  bool clear(std::uint8_t slot);
  bool text(std::uint8_t slot, std::uint16_t x, std::uint16_t baselineY,
            std::uint8_t fontId, std::string_view value);
  const RemoteDisplayCommit *commit(std::uint32_t revision);
  void cancel();

  std::uint32_t revision() const { return revision_; }
  std::optional<std::uint32_t> stagedRevision() const;
  const std::optional<RemoteDisplayScene> &committed() const {
    return committed_;
  }
  const std::optional<RemoteDisplayCommit> &lastCommit() const {
    return lastCommit_;
  }

 private:
  struct StagedTransaction : RemoteDisplayScene {
    DisplayMode mode = DisplayMode::Full;
  };

  DisplayRegionState *findStagedRegion(std::uint8_t slot);
  bool stagedContainsSlot(std::uint8_t slot) const;
  bool buildCandidate();
  void appendDirty(DisplayRect bounds, bool &overflowed);
  bool reject();

  std::uint32_t revision_ = 0;
  std::optional<StagedTransaction> staged_;
  std::optional<RemoteDisplayScene> committed_;
  RemoteDisplayScene candidate_;
  std::optional<RemoteDisplayCommit> lastCommit_;
};

static_assert(sizeof(RemoteDisplay) <= 5000,
              "remote display state must remain bounded global storage");

std::optional<std::string> decodeDisplayText(std::string_view encoded);
std::string formatDisplayOk(std::uint32_t revision);
std::string formatDisplayResync(std::uint32_t currentRevision);
std::string formatDisplayError(std::uint32_t revision, std::string_view code);
std::optional<std::string> dispatchDisplayCommand(RemoteDisplay &display,
                                                  const HelperCommand &command,
                                                  bool displaySupported);
std::optional<std::string> discardMalformedDisplayCommand(
    RemoteDisplay &display, std::string_view line);
