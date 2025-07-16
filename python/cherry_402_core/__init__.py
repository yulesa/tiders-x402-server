from enum import Enum
from cherry_402_python import *

class Network(str, Enum):
    BASE_SEPOLIA = "base_sepolia"
    BASE = "base"
    XDC_MAINNET = "xdc_mainnet"
    AVALANCHE_FUJI = "avalanche_fuji"
    AVALANCHE = "avalanche"