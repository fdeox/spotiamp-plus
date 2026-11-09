// @ts-nocheck: generated (scratchpad classnames.py)
// Spotiamp+: the engine names its page elements (<container>, <layout>,
// <button>...) and MAKI's getClassName after its classes' names, which a
// minified build shortens; its styles and scripts go by the real names.
// Each class gets its own name back here, before any skin is drawn.
import AlbumArt from "./makiClasses/AlbumArt";
import AnimatedLayer from "./makiClasses/AnimatedLayer";
import Application from "./makiClasses/Application";
import Avs from "./makiClasses/Avs";
import BaseObject from "./makiClasses/BaseObject";
import BitList from "./makiClasses/BitList";
import Browser from "./makiClasses/Browser";
import Button from "./makiClasses/Button";
import CfgGroup from "./makiClasses/CfgGroup";
import CheckBox from "./makiClasses/CheckBox";
import ComponentBucket from "./makiClasses/ComponentBucket";
import Config from "./makiClasses/Config";
import ConfigAttribute from "./makiClasses/ConfigAttribute";
import ConfigItem from "./makiClasses/ConfigItem";
import Container from "./makiClasses/Container";
import DropDownList from "./makiClasses/DropDownList";
import Edit from "./makiClasses/Edit";
import EqVis from "./makiClasses/EqVis";
import File from "./makiClasses/File";
import Frame from "./makiClasses/Frame";
import Grid from "./makiClasses/Grid";
import Group from "./makiClasses/Group";
import GroupList from "./makiClasses/GroupList";
import GroupXFade from "./makiClasses/GroupXFade";
import GuiList from "./makiClasses/GuiList";
import GuiObj from "./makiClasses/GuiObj";
import GuiTree from "./makiClasses/GuiTree";
import Images from "./makiClasses/Images";
import Layer from "./makiClasses/Layer";
import Layout from "./makiClasses/Layout";
import LayoutStatus from "./makiClasses/LayoutStatus";
import MakiList from "./makiClasses/List";
import MakiMap from "./makiClasses/MakiMap";
import Menu from "./makiClasses/Menu";
import MouseRedir from "./makiClasses/MouseRedir";
import NStateButton from "./makiClasses/NStateButton";
import { PlEdit, PlDir } from "./makiClasses/PlayList";
import PlayListGui from "./makiClasses/PlayListGui";
import PopupMenu from "./makiClasses/PopupMenu";
import ProgressGrid from "./makiClasses/ProgressGrid";
import QueryList from "./makiClasses/QueryList";
import Rect from "./makiClasses/Rect";
import Region from "./makiClasses/Region";
import Slider, { ActionHandler } from "./makiClasses/Slider";
import SnapPoint from "./makiClasses/SnapPoint";
import Status from "./makiClasses/Status";
import SystemObject from "./makiClasses/SystemObject";
import TabSheet from "./makiClasses/TabSheet";
import Text, { DisplayHandler } from "./makiClasses/Text";
import Timer from "./makiClasses/Timer";
import ToggleButton from "./makiClasses/ToggleButton";
import TreeItem from "./makiClasses/TreeItem";
import Vis, { VisPaintHandler } from "./makiClasses/Vis";
import Wac from "./makiClasses/Wac";
import WasabiButton from "./makiClasses/WasabiButton";
import WasabiTitleBar from "./makiClasses/WasabiTitle";
// (and the engine's own outside that folder)
import ColorThemesList from "./ColorThemesList";
import AudioEventedGui from "./AudioEventedGui";
import WinampConfig, { WinampConfigGroup } from "./makiClasses/WinampConfig";
import WindowHolder from "./makiClasses/WindowHolder";
import XmlDoc from "./makiClasses/XmlDoc";

const CLASSES = {
  ColorThemesList,
  AudioEventedGui,
  AlbumArt,
  AnimatedLayer,
  Application,
  Avs,
  BaseObject,
  BitList,
  Browser,
  Button,
  CfgGroup,
  CheckBox,
  ComponentBucket,
  Config,
  ConfigAttribute,
  ConfigItem,
  Container,
  DropDownList,
  Edit,
  EqVis,
  File,
  Frame,
  Grid,
  Group,
  GroupList,
  GroupXFade,
  GuiList,
  GuiObj,
  GuiTree,
  Images,
  Layer,
  Layout,
  LayoutStatus,
  MakiList,
  MakiMap,
  Menu,
  MouseRedir,
  NStateButton,
  PlEdit,
  PlDir,
  PlayListGui,
  PopupMenu,
  ProgressGrid,
  QueryList,
  Rect,
  Region,
  Slider,
  ActionHandler,
  SnapPoint,
  Status,
  SystemObject,
  TabSheet,
  Text,
  DisplayHandler,
  Timer,
  ToggleButton,
  TreeItem,
  Vis,
  VisPaintHandler,
  Wac,
  WasabiButton,
  WasabiTitleBar,
  WinampConfig,
  WinampConfigGroup,
  WindowHolder,
  XmlDoc,
};
for (const [name, cls] of Object.entries(CLASSES)) {
  if (typeof cls === "function" && cls.name !== name) Object.defineProperty(cls, "name", { value: name });
}
