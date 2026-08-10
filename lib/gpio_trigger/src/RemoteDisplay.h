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

enum class DisplayMode { Full, Delta };
enum class DisplayResult { Accepted, Resync, Rejected };
enum class DisplayOperationKind { Clear, Text };

struct DisplayRect {
  std::uint16_t x;
  std::uint16_t y;
  std::uint16_t width;
  std::uint16_t height;
};

struct DisplayTextOp {
  std::uint8_t slot;
  std::uint16_t x;
  std::uint16_t baselineY;
  std::uint8_t fontId;
  std::string text;
};

struct DisplayRegionState {
  std::uint8_t slot = 0;
  DisplayRect bounds{};
  std::array<DisplayOperationKind, kMaxDisplayOps> operations{};
  std::array<std::uint8_t, kMaxDisplayOps> operationTextIndexes{};
  std::size_t operationCount = 0;
  std::array<DisplayTextOp, kMaxDisplayOps> textOps{};
  std::size_t textOpCount = 0;
};

struct RemoteDisplayScene {
  std::uint32_t revision = 0;
  std::array<DisplayRegionState, kMaxDisplayRegions> regions{};
  std::size_t regionCount = 0;
};

struct RemoteDisplayCommit {
  std::uint32_t revision = 0;
  bool full = false;
  std::array<DisplayRegionState, kMaxDisplayRegions> regions{};
  std::size_t regionCount = 0;
  std::array<DisplayRect, kMaxDisplayRegions> dirtyBounds{};
  std::size_t dirtyCount = 0;
};

class RemoteDisplay {
 public:
  DisplayResult begin(std::uint32_t newRevision, std::uint32_t baseRevision,
                      DisplayMode mode);
  bool region(std::uint8_t slot, DisplayRect bounds);
  bool clear(std::uint8_t slot);
  bool text(std::uint8_t slot, std::uint16_t x, std::uint16_t baselineY,
            std::uint8_t fontId, std::string_view value);
  std::optional<RemoteDisplayCommit> commit(std::uint32_t revision);
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
  struct StagedTransaction {
    std::uint32_t revision = 0;
    DisplayMode mode = DisplayMode::Full;
    std::array<DisplayRegionState, kMaxDisplayRegions> regions{};
    std::size_t regionCount = 0;
    std::size_t operationCount = 0;
  };

  DisplayRegionState *findStagedRegion(std::uint8_t slot);
  bool reject();

  std::uint32_t revision_ = 0;
  std::optional<StagedTransaction> staged_;
  std::optional<RemoteDisplayScene> committed_;
  std::optional<RemoteDisplayCommit> lastCommit_;
};

std::optional<std::string> decodeDisplayText(std::string_view encoded);
std::string formatDisplayOk(std::uint32_t revision);
std::string formatDisplayResync(std::uint32_t currentRevision);
std::string formatDisplayError(std::uint32_t revision, std::string_view code);
std::optional<std::string> dispatchDisplayCommand(RemoteDisplay &display,
                                                  const HelperCommand &command,
                                                  bool displaySupported);
std::optional<std::string> discardMalformedDisplayCommand(
    RemoteDisplay &display, std::string_view line);
